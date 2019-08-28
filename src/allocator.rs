/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use lazy_static::lazy_static;
use log::debug;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use shared_memory::LockType;
use shared_memory::SharedMem;
use std::iter;
use std::mem;
use std::ptr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicPtr;
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

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

const MAX_SHMEMS: usize = 10_000;
const MIN_OBJECT_SIZE: usize = 8;

struct ShmemMetadata {
    num_shmems: AtomicUsize,
    shmem_used: [AtomicBool; MAX_SHMEMS],
    shmem_names: [ShmemName; MAX_SHMEMS],
    unused: AtomicSharedAddress,
    // TODO: add free lists
}

pub struct ShmemAllocator {
    // These fields are local to this process
    shmem: SharedMem,
    shmems: *mut AtomicPtr<SharedMem>,
    // The rest are stored in the shared memory
    num_shmems: *mut AtomicUsize,
    shmem_used: *mut AtomicBool,
    shmem_names: *mut ShmemName,
    unused: *mut AtomicSharedAddress,
}

unsafe impl Sync for ShmemAllocator {}
unsafe impl Send for ShmemAllocator {}

impl ShmemAllocator {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub unsafe fn from_shmem(shmem: SharedMem) -> ShmemAllocator {
        let metadata = shmem.get_ptr() as *mut ShmemMetadata;
        let num_shmems = &mut (*metadata).num_shmems;
        let shmem_used = &mut (*metadata).shmem_used[0];
        let shmem_names = &mut (*metadata).shmem_names[0];
        let unused = &mut (*metadata).unused;
        let mut shmem_vec: Vec<AtomicPtr<SharedMem>> =
            iter::repeat_with(|| AtomicPtr::new(ptr::null_mut()))
                .take(MAX_SHMEMS)
                .collect();
        let shmems = shmem_vec.as_mut_ptr();
        mem::forget(shmem_vec);
        ShmemAllocator {
            shmem,
            shmems,
            num_shmems,
            shmem_used,
            shmem_names,
            unused,
        }
    }

    pub fn create() -> Option<ShmemAllocator> {
        let size = mem::size_of::<ShmemMetadata>();
        let shmem = SharedMem::create(LockType::Mutex, size).ok()?;
        unsafe { shmem.get_ptr().write_bytes(0, size) };
        Some(unsafe { ShmemAllocator::from_shmem(shmem) })
    }

    pub fn open(name: &str) -> Option<ShmemAllocator> {
        let shmem = SharedMem::open(name).ok()?;
        Some(unsafe { ShmemAllocator::from_shmem(shmem) })
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn shmem(&self) -> &SharedMem {
        &self.shmem
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn get_num_shmems(&self) -> usize {
        unsafe { &*self.num_shmems }.load(Ordering::SeqCst)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    unsafe fn get_shmem_name(&self, shmem_id: ShmemId) -> Option<&ShmemName> {
        if shmem_id.to_usize()? > self.get_num_shmems() {
            None
        } else {
            Some(&*self.shmem_names.offset(shmem_id.to_isize()?))
        }
    }

    // I'd like to be able to mark this as `no_panic` but unfortunately
    // the shared memory crate can panic when opening a shared memory file.
    unsafe fn get_shmem(&self, shmem_id: ShmemId) -> Option<&SharedMem> {
        let atomic_shmem = &*self.shmems.offset(shmem_id.to_isize()?);
        if let Some(shmem) = atomic_shmem.load(Ordering::SeqCst).as_ref() {
            return Some(shmem);
        }
        let shmem_name = self.get_shmem_name(shmem_id)?;
        let mut new_shmem = Box::new(SharedMem::open(shmem_name.as_str()).ok()?);
        let new_shmem_ptr = &mut *new_shmem as *mut _;
        if let Some(new_new_shmem) = atomic_shmem
            .compare_and_swap(ptr::null_mut(), new_shmem_ptr, Ordering::SeqCst)
            .as_ref()
        {
            return Some(new_new_shmem);
        }
        mem::forget(new_shmem);
        Some(&*new_shmem_ptr)
    }

    // I'd like to be able to mark this as `no_panic` but unfortunately
    // the shared memory crate can panic when creating a shared memory file.
    unsafe fn alloc_shmem(&self, size: usize) -> Option<ShmemId> {
        let mut shmem = Box::new(SharedMem::create(LockType::Mutex, size).ok()?);
        let shmem_ptr = &mut *shmem as *mut SharedMem;
        let shmem_name = ShmemName::from_str(shmem.get_os_path())?;
        let mut index = (&*self.num_shmems).load(Ordering::Relaxed);
        while (&*self.shmem_used.offset(index as isize)).swap(true, Ordering::SeqCst) {
            if MAX_SHMEMS <= index {
                return None;
            } else {
                index += 1;
            }
        }
        debug!(
            "Allocated shmem {} of size {} (requested {:?})",
            index,
            (&*shmem_ptr).get_size(),
            size
        );
        *self.shmem_names.offset(index as isize) = shmem_name;
        if (&*self.shmems.offset(index as isize))
            .compare_and_swap(ptr::null_mut(), shmem_ptr, Ordering::SeqCst)
            .is_null()
        {
            mem::forget(shmem);
        } else {
            debug!("Another thread has already opened shmem {}", index);
        }
        (&*self.num_shmems).fetch_add(1, Ordering::SeqCst);
        ShmemId::from_usize(index)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    unsafe fn free_shmem(&self, _shmem_id: ShmemId) {
        // TODO
    }

    pub fn get_bytes(&self, address: SharedAddressRange) -> Option<*mut u8> {
        let shmem = unsafe { self.get_shmem(address.shmem_id()) }?;
        let object_offset = address.object_offset().to_isize()?;
        let object_end = address.object_end()?.to_usize()?;
        if object_end > shmem.get_size() {
            None
        } else {
            Some(unsafe { shmem.get_ptr().offset(object_offset) as *mut u8 })
        }
    }

    pub unsafe fn alloc_bytes(&self, size: usize) -> Option<SharedAddressRange> {
        let object_size = ObjectSize::ceil(usize::max(MIN_OBJECT_SIZE, size));
        loop {
            if let Some(result) = (*self.unused).fetch_add(object_size, Ordering::SeqCst) {
                return Some(result);
            }
            let old_unused = (*self.unused).load(Ordering::SeqCst);
            let old_shmem = unsafe { self.get_shmem(old_unused.shmem_id()) };
            let old_shmem_size = old_shmem.map(|shmem| shmem.get_size()).unwrap_or(0);
            let new_shmem_size = ObjectSize::max(object_size, ObjectSize::ceil(old_shmem_size + 1));
            let new_shmem_id = self.alloc_shmem(new_shmem_size.to_usize()?)?;
            let object_offset = ObjectOffset::from_u64(0)?;
            let new_unused = SharedAddress::new(new_shmem_id, new_shmem_size, object_offset);
            if old_unused
                != (*self.unused).compare_and_swap(old_unused, new_unused, Ordering::SeqCst)
            {
                self.free_shmem(new_shmem_id);
            }
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub unsafe fn free_bytes(&self, _addr: SharedAddressRange) {
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
