use shared_memory::SharedMem;
use shared_memory::LockType;
use std::num::NonZeroUsize;
use std::mem;
use std::ptr;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;

const MAX_SHMEMS: usize = 10_000;
const SHMEM_SIZE: usize = 1_000_000;

struct ShmemAllocator {
    shmem: SharedMem,
    num_shmems: *mut AtomicUsize,
    shmem_free: *mut AtomicBool,
    shmem_names: *mut ShmemName,
    shmems: *mut AtomicPtr<SharedMem>,

    // unused: *mut Option<SharedAddress>,
    // shmems: *mut AtomicPtr<u8>,
}

impl ShmemAllocator {
    fn get_num_shmems(&self) -> usize {
        unsafe { &*self.num_shmems }.load(Ordering::SeqCst)
    }

    unsafe fn get_shmem_name(&self, shmem_id: ShmemId) -> Option<&ShmemName> {
        if (shmem_id.0 as usize) > self.get_num_shmems() {
            None
        } else {
            Some(&*self.shmem_names.offset(shmem_id.0 as isize))
        }
    }

    unsafe fn get_shmem(&self, shmem_id: ShmemId) -> Option<&SharedMem> {
        let atomic_shmem = &*self.shmems.offset(shmem_id.0 as isize);
        if let Some(shmem) = atomic_shmem.load(Ordering::SeqCst).as_ref() {
            return Some(shmem);
        }
        let shmem_name = self.get_shmem_name(shmem_id)?;
        let mut new_shmem = Box::new(SharedMem::open(shmem_name.as_str()).ok()?);
        let new_shmem_ptr = &mut *new_shmem as *mut _;
        if let Some(new_new_shmem) = atomic_shmem.compare_and_swap(ptr::null_mut(), new_shmem_ptr, Ordering::SeqCst).as_ref() {
            return Some(new_new_shmem);
        }
        mem::forget(new_shmem);
        Some(&*new_shmem_ptr)
    }

    unsafe fn alloc_shmem(&self) -> Option<ShmemId> {
        let mut shmem = Box::new(SharedMem::create(LockType::RwLock, SHMEM_SIZE).ok()?);
        let shmem_ptr = &mut *shmem as *mut _;
        let shmem_name = ShmemName::from_str(shmem.get_os_path())?;
        let mut index = (&*self.num_shmems).load(Ordering::Relaxed);
        while (&*self.shmem_free.offset(index as isize)).swap(true, Ordering::SeqCst) {
            if MAX_SHMEMS <= index {
                return None;
            } else {
                index += 1;
            }
        }
        *self.shmem_names.offset(index as isize) = shmem_name;
        if (&*self.shmems.offset(index as isize)).compare_and_swap(ptr::null_mut(), shmem_ptr, Ordering::SeqCst).is_null() {
            mem::forget(shmem);
        }
        (&*self.num_shmems).fetch_add(1, Ordering::SeqCst);
        Some(ShmemId(index as u32))
    }
}

struct SharedAddress(NonZeroUsize);

struct ShmemId(u32);

struct ShmemName([u8; 16]);

impl ShmemName {
    fn from_str(name: &str) -> Option<Self> {
        unimplemented!()
    }
    fn as_str(&self) -> &str {
        unimplemented!()
    }
}

struct Offset(u32);

impl SharedAddress {
    fn shmem(&self) -> ShmemId {
        ShmemId((self.0.get() >> 32) as u32)
    }

    fn offset(&self) -> Offset {
        Offset(self.0.get() as u32)
    }
}