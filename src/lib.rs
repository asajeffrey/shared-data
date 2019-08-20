use arrayvec::ArrayString;
use lazy_static::lazy_static;
use shared_memory::SharedMem;
use shared_memory::SharedMemCast;
use shared_memory::LockType;
use std::num::NonZeroU8;
use std::num::NonZeroU64;
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use std::ptr::NonNull;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;

const MAX_SHMEMS: usize = 10_000;
const MIN_OBJECT_SIZE: usize = 8;

struct ShmemMetadata {
    num_shmems: AtomicUsize,
    shmem_free: [AtomicBool; MAX_SHMEMS],
    shmem_names: [ShmemName; MAX_SHMEMS],
}

struct ShmemAllocator {
    shmem: SharedMem,
    num_shmems: *mut AtomicUsize,
    shmem_free: *mut AtomicBool,
    shmem_names: *mut ShmemName,
    shmems: *mut AtomicPtr<SharedMem>,
    unused: *mut AtomicSharedAddress,
}

unsafe impl Sync for ShmemAllocator {}
unsafe impl Send for ShmemAllocator {}

impl ShmemAllocator {
    unsafe fn from_shmem(shmem: SharedMem) -> ShmemAllocator {
        let metadata = shmem.get_ptr() as *mut ShmemMetadata;
        let num_shmems = &mut (*metadata).num_shmems;
        let shmem_free = &mut (*metadata).shmem_free[0];
        let shmem_names = &mut (*metadata).shmem_names[0];
        let shmems = Box::into_raw(Box::new(Default::default()));
        let unused = Box::into_raw(Box::new(Default::default()));
        ShmemAllocator { shmem, num_shmems, shmem_free, shmem_names, shmems, unused }
    }

    fn create() -> Option<ShmemAllocator> {
        let size = mem::size_of::<ShmemMetadata>();
        let shmem = SharedMem::create(LockType::RwLock, size).ok()?;
        unsafe { shmem.get_ptr().write_bytes(0, size) };
        Some(unsafe { ShmemAllocator::from_shmem(shmem) })
    }

    fn open(name: &str) -> Option<ShmemAllocator> {
        let shmem = SharedMem::open(name).ok()?;
        Some(unsafe { ShmemAllocator::from_shmem(shmem) })
    }

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

    unsafe fn alloc_shmem(&self, size: usize) -> Option<ShmemId> {
        let mut shmem = Box::new(SharedMem::create(LockType::RwLock, size).ok()?);
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
        Some(ShmemId(index as u16))
    }

    unsafe fn free_shmem(&self, shmem_id: ShmemId) {
        // TODO
    }

    fn get_bytes(&self, address: SharedAddress) -> Option<NonNull<u8>> {
        let shmem = unsafe { self.get_shmem(address.shmem_id()) }?;
        let shmem_ptr = NonNull::new(shmem.get_ptr() as *mut u8)?.as_ptr();
        let object_offset = address.object_offset().as_isize();
        let object_ptr = unsafe { shmem_ptr.offset(object_offset) };
        NonNull::new(object_ptr)
    }

    unsafe fn alloc_bytes(&self, size: usize) -> Option<SharedAddress> {
        let object_size = ObjectSize::ceil(size);
        let atomic_unused = &*self.unused.offset(object_size.0.get() as isize);
        loop {
            let mut old_size = 0;
            let unused = atomic_unused.fetch_add(object_size.as_offset(), Ordering::SeqCst);
              if let Some(unused) = unused {
                if let Some(shmem) = self.get_shmem(unused.shmem_id()) {
                    old_size = shmem.get_size();
                    if unused.object_end().as_usize() <= old_size {
                        return Some(unused);
                    }
                }
            }
            let new_shmem_size = usize::max(size, old_size * 2);
            let new_shmem_id = self.alloc_shmem(new_shmem_size)?;
            let result = SharedAddress::new(new_shmem_id, object_size, ObjectOffset(0));
            let new_unused = Some(SharedAddress::new(new_shmem_id, object_size, object_size.as_offset()));
            if unused == atomic_unused.compare_and_swap(unused, new_unused, Ordering::SeqCst) {
                return Some(result);
            } else {
                self.free_shmem(new_shmem_id);
            }
        }
    }

    unsafe fn free_bytes(&self, addr: SharedAddress) {
        // TODO
    }
}

#[repr(C)]
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
struct RawSharedAddress {
    shmem_id: u16,
    object_size: u8,
    padding: u8,
    object_offset: u32,
}

impl RawSharedAddress {
    fn from_u64(bits: u64) -> RawSharedAddress {
        unsafe { mem::transmute(bits) }
    }

    fn to_u64(self) -> u64 {
        unsafe { mem::transmute(self) }
    }

    fn is_valid(self) -> bool {
        (self.object_size != 0) && (self.padding == 0)
    }
}

#[derive(Clone, Copy, Eq, Debug, PartialEq)]
struct SharedAddress(NonZeroU64);

impl SharedAddress {
    unsafe fn from_raw_unchecked(raw: RawSharedAddress) -> SharedAddress {
        SharedAddress(NonZeroU64::new_unchecked(raw.to_u64()))
    }

    fn from_raw(raw: RawSharedAddress) -> Option<SharedAddress> {
        if raw.is_valid() {
            Some(unsafe { SharedAddress::from_raw_unchecked(raw) })
        } else {
            None
        }
    }

    fn as_raw(self) -> RawSharedAddress {
        RawSharedAddress::from_u64(self.0.get())
    }

    fn new(shmem_id: ShmemId, size: ObjectSize, offset: ObjectOffset) -> SharedAddress {
        unsafe {
            SharedAddress::from_raw_unchecked(RawSharedAddress {
                shmem_id: shmem_id.0,
                object_size: size.0.get(),
                padding: 0,
                object_offset: offset.0,
            })
        }
    }

    fn shmem_id(self) -> ShmemId {
        ShmemId(self.as_raw().shmem_id)
    }

    fn object_size(&self) -> ObjectSize {
        ObjectSize(unsafe { NonZeroU8::new_unchecked(self.as_raw().object_size) })
    }

    fn object_offset(&self) -> ObjectOffset {
        ObjectOffset(self.as_raw().object_offset)
    }

    fn object_end(&self) -> ObjectOffset {
        self.object_offset() + self.object_size().as_offset()
    }
}

unsafe impl SharedMemCast for SharedAddress {}

#[derive(Default)]
struct AtomicSharedAddress(AtomicU64);

impl AtomicSharedAddress {
    fn compare_and_swap(&self, current: Option<SharedAddress>, new: Option<SharedAddress>, order: Ordering) -> Option<SharedAddress> {
        let current = current.map(|addr| addr.0.get()).unwrap_or(0);
        let new = new.map(|addr| addr.0.get()).unwrap_or(0);
        let bits = self.0.compare_and_swap(current, new, order);
        SharedAddress::from_raw(RawSharedAddress::from_u64(bits))
    }
    fn fetch_add(&self, offset: ObjectOffset, order: Ordering) -> Option<SharedAddress> {
        let bits = self.0.fetch_add(offset.as_u64(), order);
        let result = SharedAddress::from_raw(RawSharedAddress::from_u64(bits));
        if result.is_none() { self.0.fetch_sub(offset.0 as u64, order); }
        result
    }
}

#[derive(Clone, Copy, Eq, Debug, Ord, PartialEq, PartialOrd)]
struct ObjectSize(NonZeroU8);

impl ObjectSize {
    fn as_offset(&self) -> ObjectOffset {
        ObjectOffset(1 << self.0.get())
    }

    fn ceil(size: usize) -> ObjectSize {
        let size = usize::max(size, MIN_OBJECT_SIZE);
        ObjectSize(unsafe { NonZeroU8::new_unchecked(64 - size.leading_zeros() as u8) })
    }

    fn floor(size: usize) -> ObjectSize {
        let size = usize::max(size, MIN_OBJECT_SIZE);
        ObjectSize(unsafe { NonZeroU8::new_unchecked(63 - (size + 1).leading_zeros() as u8) })
    }
}

#[derive(Clone, Copy, Eq, Debug, Ord, PartialEq, PartialOrd)]
struct ObjectOffset(u32);

impl ObjectOffset {
    fn as_u64(self) -> u64 {
        self.0 as u64
    }

    fn as_usize(self) -> usize {
        self.0 as usize
    }

    fn as_isize(self) -> isize {
        self.0 as isize
    }
}

impl std::ops::Add for ObjectOffset {
    type Output = ObjectOffset;
    fn add(self, rhs: ObjectOffset) -> ObjectOffset {
        ObjectOffset(self.0 + rhs.0)
    }
}

#[derive(Clone, Copy, Eq, Debug, PartialEq)]
struct ShmemId(u16);

struct ShmemName(ArrayString<[u8; 16]>);

impl ShmemName {
    fn from_str(name: &str) -> Option<Self> {
        let name = ArrayString::from(name).ok()?;
        Some(ShmemName(name))
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

struct Offset(u32);

lazy_static! {
   static ref ALLOCATOR_NAME: RwLock<Option<String>> = RwLock::new(None);
   static ref ALLOCATOR: ShmemAllocator = {
       let name = ALLOCATOR_NAME.read().expect("Failed to lock");
       if let Some(ref name) = *name {
           ShmemAllocator::open(&*name).expect(&format!("Failed to open shared memory {}.", name))
       } else {
           ShmemAllocator::create().expect("Failed to create shared memory")
       }
   };
}

pub struct SharedBox<T> {
    address: SharedAddress,
    marker: PhantomData<T>,
}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedBox<T> {}
unsafe impl<T: Sync> Sync for SharedBox<T> {}
unsafe impl<T: Send> Send for SharedBox<T> {}

impl<T> SharedBox<T> {
    pub fn new(data: T) -> SharedBox<T> {
        let size = mem::size_of::<T>();
        let address = unsafe { ALLOCATOR.alloc_bytes(size) }.expect("Failed to allocate shared box");
        let marker = PhantomData;
        SharedBox { address, marker }
    }

    pub fn as_ptr(&self) -> Option<NonNull<T>> {
        let ptr = ALLOCATOR.get_bytes(self.address)?;
        Some(ptr.cast())
    }
}

impl<T> Drop for SharedBox<T> {
    fn drop(&mut self) {
        unsafe {
            if let Some(ptr) = self.as_ptr() {
                ptr.as_ptr().read();
            }
            ALLOCATOR.free_bytes(self.address);
        }
    }
}
