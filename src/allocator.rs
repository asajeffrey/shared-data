/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use array_macro::array;
use atom::AtomSetOnce;
use lazy_static::lazy_static;
use log::debug;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use owning_ref::BoxRef;
use owning_ref::OwningRef;
use shared_memory::LockType;
use shared_memory::SharedMem;
use std::mem;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use crate::AtomicSharedAddress;
use crate::ObjectOffset;
use crate::ObjectSize;
use crate::SharedAddress;
use crate::SharedAddressRange;
use crate::ShmemId;
use crate::ShmemName;
use crate::SyncSharedMem;
use crate::Volatile;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

// We double the size of the shared blocks each time we allocate one,
// so we will run out of memory a long time before we run out of shared
// memory blocks.
const MAX_SHMEMS: usize = 64;
const MIN_OBJECT_SIZE: usize = 8;

pub(crate) struct ShmemMetadata {
    name: Volatile<ShmemName>,
    num_shmems: AtomicUsize,
    shmem_used: [AtomicBool; MAX_SHMEMS],
    shmem_names: [Volatile<ShmemName>; MAX_SHMEMS],
    unused: AtomicSharedAddress,
    // TODO: add free lists
}

impl ShmemMetadata {
    fn new(name: ShmemName) -> ShmemMetadata {
        ShmemMetadata {
            name: Volatile::new(name),
            num_shmems: AtomicUsize::new(0),
            shmem_used: array![AtomicBool::new(false); MAX_SHMEMS],
            shmem_names: array![Volatile::new(ShmemName::default()); MAX_SHMEMS],
            unused: AtomicSharedAddress::default(),
        }
    }
}

pub struct ShmemAllocator {
    // Locally we store the mmap'd memory slices
    shmems: [AtomSetOnce<Box<SyncSharedMem>>; MAX_SHMEMS],
    // The metadata is stored in shared memory
    metadata_shmem: BoxRef<SyncSharedMem, ShmemMetadata>,
}

impl ShmemAllocator {
    pub fn from_shmem(shmem: SyncSharedMem) -> Option<ShmemAllocator> {
        let metadata_shmem = OwningRef::new(Box::new(shmem))
            .try_map(|bytes| {
                Volatile::<ShmemMetadata>::from_volatile_bytes(bytes)
                    .map(|metadata| metadata.deref())
                    .ok_or(())
            })
            .ok()?;
        Some(ShmemAllocator {
            shmems: array![AtomSetOnce::empty(); MAX_SHMEMS],
            metadata_shmem,
        })
    }

    pub fn create() -> Option<ShmemAllocator> {
        let size = mem::size_of::<ShmemMetadata>();
        let shmem = SharedMem::create(LockType::Mutex, size).ok()?;
        let shmem_name = ShmemName::from_str(shmem.get_os_path())?;
        let shmem = SyncSharedMem::from_shmem(shmem);
        let metadata = ShmemMetadata::new(shmem_name);
        let volatile_metadata = Volatile::<ShmemMetadata>::from_volatile_bytes(&*shmem)?;
        volatile_metadata.write_volatile(metadata);
        ShmemAllocator::from_shmem(shmem)
    }

    pub fn open(name: &str) -> Option<ShmemAllocator> {
        let shmem = SharedMem::open(name).ok()?;
        let shmem = SyncSharedMem::from_shmem(shmem);
        ShmemAllocator::from_shmem(shmem)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn metadata(&self) -> &ShmemMetadata {
        &*self.metadata_shmem
    }

    pub fn name(&self) -> ShmemName {
        self.metadata().name.read_volatile()
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn get_num_shmems(&self) -> usize {
        self.metadata().num_shmems.load(Ordering::SeqCst)
    }

    fn get_shmem_name(&self, shmem_id: ShmemId) -> Option<ShmemName> {
        let index = shmem_id.to_usize()?;
        if index > self.get_num_shmems() {
            None
        } else if !self
            .metadata()
            .shmem_used
            .get(index)?
            .load(Ordering::SeqCst)
        {
            None
        } else {
            Some(self.metadata().shmem_names.get(index)?.read_volatile())
        }
    }

    // I'd like to be able to mark this as `no_panic` but unfortunately
    // the shared memory crate can panic when opening a shared memory file.
    fn get_shmem(&self, shmem_id: ShmemId) -> Option<&SyncSharedMem> {
        let index = shmem_id.to_usize()?;
        let atomic_shmem = self.shmems.get(index)?;
        if let Some(shmem) = atomic_shmem.get() {
            return Some(shmem);
        }
        let shmem_name = self.get_shmem_name(shmem_id)?;
        let new_shmem = SharedMem::open(shmem_name.as_str()).ok()?;
        let new_boxed_shmem = Box::new(SyncSharedMem::from_shmem(new_shmem));
        atomic_shmem.set_if_none(new_boxed_shmem);
        atomic_shmem.get()
    }

    // I'd like to be able to mark this as `no_panic` but unfortunately
    // the shared memory crate can panic when creating a shared memory file.
    fn alloc_shmem(&self, size: usize) -> Option<ShmemId> {
        let shmem = SharedMem::create(LockType::Mutex, size).ok()?;
        let shmem_name = ShmemName::from_str(shmem.get_os_path())?;
        let boxed_shmem = Box::new(SyncSharedMem::from_shmem(shmem));
        let mut index = self.metadata().num_shmems.load(Ordering::Relaxed);
        while self
            .metadata()
            .shmem_used
            .get(index)?
            .swap(true, Ordering::SeqCst)
        {
            if MAX_SHMEMS <= index {
                return None;
            } else {
                index += 1;
            }
        }
        debug!(
            "Allocated shmem {} of size {} (requested {:?})",
            index,
            boxed_shmem.len(),
            size
        );
        self.metadata()
            .shmem_names
            .get(index)?
            .write_volatile(shmem_name);
        self.shmems.get(index)?.set_if_none(boxed_shmem);
        self.metadata().num_shmems.fetch_add(1, Ordering::SeqCst);
        ShmemId::from_usize(index)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn free_shmem(&self, _shmem_id: ShmemId) {
        // TODO
    }

    pub fn get_bytes(&self, address: SharedAddressRange) -> Option<&[Volatile<u8>]> {
        let shmem = self.get_shmem(address.shmem_id())?;
        let object_offset = address.object_offset().to_usize()?;
        let object_end = address.object_end()?.to_usize()?;
        if object_end > shmem.len() {
            None
        } else {
            Some(&shmem[object_offset..object_end])
        }
    }

    pub fn alloc_bytes(&self, size: usize) -> Option<SharedAddressRange> {
        let object_size = ObjectSize::ceil(usize::max(MIN_OBJECT_SIZE, size));
        loop {
            if let Some(result) = self
                .metadata()
                .unused
                .fetch_add(object_size, Ordering::SeqCst)
            {
                return Some(result);
            }
            let old_unused = self.metadata().unused.load(Ordering::SeqCst);
            let old_shmem_size = self
                .get_shmem(old_unused.shmem_id())
                .map(|shmem| shmem.len())
                .unwrap_or(0);
            let new_shmem_size = ObjectSize::max(object_size, ObjectSize::ceil(old_shmem_size + 1));
            let new_shmem_id = self.alloc_shmem(new_shmem_size.to_usize()?)?;
            let object_offset = ObjectOffset::from_u64(0)?;
            let new_unused = SharedAddress::new(new_shmem_id, new_shmem_size, object_offset);
            if old_unused
                != self
                    .metadata()
                    .unused
                    .compare_and_swap(old_unused, new_unused, Ordering::SeqCst)
            {
                self.free_shmem(new_shmem_id);
            }
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn free_bytes(&self, _addr: SharedAddressRange) {
        // TODO
    }
}

lazy_static! {
    pub static ref ALLOCATOR_NAME: Mutex<Option<String>> = Mutex::new(None);
    pub static ref ALLOCATOR: ShmemAllocator = {
        if let Some(name) = ALLOCATOR_NAME.lock().ok().and_then(|mut name| name.take()) {
            ShmemAllocator::open(&*name).expect(&format!("Failed to open shared memory {}.", name))
        } else {
            ShmemAllocator::create().expect("Failed to create shared memory")
        }
    };
}

pub fn bootstrap(name: String) {
    if let Ok(mut allocator_name) = ALLOCATOR_NAME.lock() {
        *allocator_name = Some(name);
    }
}
