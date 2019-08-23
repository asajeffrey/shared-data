use arrayvec::ArrayString;
use lazy_static::lazy_static;
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use shared_memory::LockType;
use shared_memory::SharedMem;
use shared_memory::SharedMemCast;
use std::iter;
use std::marker::PhantomData;
use std::mem;
use std::num::NonZeroU64;
use std::num::NonZeroU8;
use std::ptr;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

const MAX_SHMEMS: usize = 10_000;
const MIN_OBJECT_SIZE: usize = 8;

struct ShmemMetadata {
    num_shmems: AtomicUsize,
    shmem_used: [AtomicBool; MAX_SHMEMS],
    shmem_names: [ShmemName; MAX_SHMEMS],
    unused: [AtomicSharedAddress; 64],
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
        let unused = &mut (*metadata).unused[0];
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
        if (shmem_id.0 as usize) > self.get_num_shmems() {
            None
        } else {
            Some(&*self.shmem_names.offset(shmem_id.0 as isize))
        }
    }

    // I'd like to be able to mark this as `no_panic` but unfortunately
    // the shared memory crate can panic when opening a shared memory file.
    unsafe fn get_shmem(&self, shmem_id: ShmemId) -> Option<&SharedMem> {
        let atomic_shmem = &*self.shmems.offset(shmem_id.0 as isize);
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
        if (&*self.shmems.offset(index as isize))
            .compare_and_swap(ptr::null_mut(), shmem_ptr, Ordering::SeqCst)
            .is_null()
        {
            mem::forget(shmem);
        }
        (&*self.num_shmems).fetch_add(1, Ordering::SeqCst);
        Some(ShmemId(index as u16))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    unsafe fn free_shmem(&self, shmem_id: ShmemId) {
        // TODO
    }

    pub fn get_bytes(&self, address: SharedAddress) -> Option<NonNull<u8>> {
        let shmem = unsafe { self.get_shmem(address.shmem_id()) }?;
        let shmem_ptr = NonNull::new(shmem.get_ptr() as *mut u8)?.as_ptr();
        let object_offset = address.object_offset().to_isize()?;
        let object_ptr = unsafe { shmem_ptr.offset(object_offset) };
        NonNull::new(object_ptr)
    }

    pub unsafe fn alloc_bytes(&self, size: usize) -> Option<SharedAddress> {
        let object_size = ObjectSize::ceil(usize::max(MIN_OBJECT_SIZE, size));
        let atomic_unused = &*self.unused.offset(object_size.0 as isize);
        loop {
            let mut old_size = 0;
            let unused = atomic_unused.fetch_add(object_size, Ordering::SeqCst);
            if let Some(unused) = unused {
                if let Some(shmem) = self.get_shmem(unused.shmem_id()) {
                    old_size = shmem.get_size();
                    let offset = unused.object_offset().to_usize()?;
                    let end = offset + unused.object_size().to_usize()?;
                    if end <= old_size {
                        return Some(unused);
                    }
                }
            }
            let new_shmem_size = usize::max(size, old_size * 2);
            let new_shmem_id = self.alloc_shmem(new_shmem_size)?;
            let result = SharedAddress::new(new_shmem_id, object_size, ObjectOffset(0));
            let new_unused = Some(SharedAddress::new(
                new_shmem_id,
                object_size,
                ObjectOffset::from_u64(object_size.to_u64()?)?,
            ));
            if unused == atomic_unused.compare_and_swap(unused, new_unused, Ordering::SeqCst) {
                return Some(result);
            } else {
                self.free_shmem(new_shmem_id);
            }
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub unsafe fn free_bytes(&self, addr: SharedAddress) {
        // TODO
    }
}

#[cfg(target_endian = "big")]
#[repr(C)]
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddress {
    shmem_id: ShmemId,
    object_size: ObjectSize,
    padding: u8,
    object_offset: ObjectOffset,
}

#[cfg(target_endian = "little")]
#[repr(C)]
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddress {
    object_offset: ObjectOffset,
    padding: u8,
    object_size: ObjectSize,
    shmem_id: ShmemId,
}

impl FromPrimitive for SharedAddress {
    fn from_u64(data: u64) -> Option<SharedAddress> {
        Some(unsafe { mem::transmute(data) })
    }

    fn from_i64(data: i64) -> Option<SharedAddress> {
        u64::from_i64(data).and_then(SharedAddress::from_u64)
    }
}

impl ToPrimitive for SharedAddress {
    fn to_u64(&self) -> Option<u64> {
        Some(unsafe { mem::transmute(*self) })
    }

    fn to_i64(&self) -> Option<i64> {
        self.to_u64().as_ref().and_then(ToPrimitive::to_i64)
    }
}

impl SharedAddress {
    #[cfg_attr(feature = "no-panic", no_panic)]
    fn new(shmem_id: ShmemId, size: ObjectSize, offset: ObjectOffset) -> SharedAddress {
        SharedAddress {
            shmem_id: shmem_id,
            object_size: size,
            padding: 0,
            object_offset: offset,
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn shmem_id(self) -> ShmemId {
        self.shmem_id
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn object_size(&self) -> ObjectSize {
        self.object_size
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn object_offset(&self) -> ObjectOffset {
        self.object_offset
    }
}

unsafe impl SharedMemCast for SharedAddress {}

#[derive(Default)]
pub struct AtomicSharedAddress(AtomicU64);

impl AtomicSharedAddress {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn compare_and_swap(
        &self,
        current: Option<SharedAddress>,
        new: Option<SharedAddress>,
        order: Ordering,
    ) -> Option<SharedAddress> {
        let current = current
            .as_ref()
            .and_then(SharedAddress::to_u64)
            .unwrap_or(0);
        let new = new.as_ref().and_then(SharedAddress::to_u64).unwrap_or(0);
        let bits = self.0.compare_and_swap(current, new, order);
        SharedAddress::from_u64(bits)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn fetch_add(&self, size: ObjectSize, order: Ordering) -> Option<SharedAddress> {
        let size = size.to_u64()?;
        let bits = self.0.fetch_add(size, order);
        let result = SharedAddress::from_u64(bits);
        if result.is_none() {
            self.0.fetch_sub(size, order);
        }
        result
    }
}

#[derive(Clone, Copy, Default, Eq, Debug, Ord, PartialEq, PartialOrd)]
struct ObjectSize(u8);

impl ToPrimitive for ObjectSize {
    fn to_u64(&self) -> Option<u64> {
        1u64.checked_shr(self.0 as u32)
    }

    fn to_i64(&self) -> Option<i64> {
        self.to_u64().as_ref().and_then(ToPrimitive::to_i64)
    }
}

impl FromPrimitive for ObjectSize {
    fn from_u64(data: u64) -> Option<ObjectSize> {
        if data.is_power_of_two() {
            Some(ObjectSize(63 - data.leading_zeros() as u8))
        } else {
            None
        }
    }

    fn from_i64(data: i64) -> Option<ObjectSize> {
        u64::from_i64(data).and_then(ObjectSize::from_u64)
    }
}

impl ObjectSize {
    #[cfg_attr(feature = "no-panic", no_panic)]
    fn ceil(size: usize) -> ObjectSize {
        ObjectSize(64 - (size - 1).leading_zeros() as u8)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn floor(size: usize) -> ObjectSize {
        ObjectSize(63 - size.leading_zeros() as u8)
    }
}

#[derive(
    Clone, Copy, Default, Eq, Debug, Ord, PartialEq, PartialOrd, FromPrimitive, ToPrimitive,
)]
struct ObjectOffset(u32);

#[derive(Clone, Copy, Default, Eq, Debug, PartialEq, FromPrimitive, ToPrimitive)]
struct ShmemId(u16);

#[derive(Clone, Eq, Debug, PartialEq)]
struct ShmemName(ArrayString<[u8; 32]>);

impl ShmemName {
    #[cfg_attr(feature = "no-panic", no_panic)]
    fn from_str(name: &str) -> Option<Self> {
        let name = ArrayString::from(name).ok()?;
        Some(ShmemName(name))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

struct Offset(u32);

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

pub struct SharedBox<T> {
    address: SharedAddress,
    marker: PhantomData<T>,
}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedBox<T> {}
unsafe impl<T: Sync> Sync for SharedBox<T> {}
unsafe impl<T: Send> Send for SharedBox<T> {}

impl<T> SharedBox<T> {
    pub fn new_in(data: T, alloc: &ShmemAllocator) -> Option<SharedBox<T>> {
        let size = mem::size_of::<T>();
        let address = unsafe { alloc.alloc_bytes(size)? };
        let ptr = alloc.get_bytes(address)?.as_ptr() as *mut T;
        unsafe { ptr.write_volatile(data) };
        let marker = PhantomData;
        Some(SharedBox { address, marker })
    }

    pub fn as_ptr_in(&self, alloc: &ShmemAllocator) -> Option<NonNull<T>> {
        let ptr = alloc.get_bytes(self.address)?;
        Some(ptr.cast())
    }

    pub fn new(data: T) -> SharedBox<T> {
        SharedBox::new_in(data, &ALLOCATOR).expect("Failed to allocate shared box")
    }

    pub fn as_ptr(&self) -> Option<NonNull<T>> {
        self.as_ptr_in(&ALLOCATOR)
    }

    pub fn address(&self) -> SharedAddress {
        self.address
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

#[test]
fn test_shared_box() {
    let boxed: SharedBox<usize> = SharedBox::new(37);
    let ptr = boxed.as_ptr().unwrap().as_ptr();
    let val = unsafe { ptr.read_volatile() };
    assert_eq!(val, 37);
}
