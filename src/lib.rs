/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use arrayvec::ArrayString;
use lazy_static::lazy_static;
use log::debug;
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
use std::ops::Deref;
use std::ptr;
use std::slice;
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
            "Allocated shmem {} of size {} (requested {})",
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
        Some(ShmemId(index as u16))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    unsafe fn free_shmem(&self, shmem_id: ShmemId) {
        // TODO
    }

    pub fn get_bytes(&self, address: SharedAddress) -> Option<*mut u8> {
        let shmem = unsafe { self.get_shmem(address.shmem_id()) }?;
        let object_offset = address.object_offset().to_isize()?;
        let object_end = object_offset as usize + address.object_size().to_usize()?;
        if object_end <= shmem.get_size() {
            Some(unsafe { shmem.get_ptr().offset(object_offset) as *mut u8 })
        } else {
            debug!("Out of range dereferncing {:?}", address);
            None
        }
    }

    pub unsafe fn alloc_bytes(&self, size: usize) -> Option<SharedAddress> {
        let object_size = ObjectSize::ceil(usize::max(MIN_OBJECT_SIZE, size));
        let atomic_unused = &*self.unused.offset(object_size.0 as isize);
        loop {
            let mut old_size = 0;
            let unused = atomic_unused.fetch_add(object_size, Ordering::SeqCst);
            let mut next_unused = None;
            if let Some(unused) = unused {
                next_unused = unused.checked_add(object_size);
                if let Some(shmem) = self.get_shmem(unused.shmem_id()) {
                    old_size = shmem.get_size();
                    if let Some(next_unused) = next_unused {
                        if let Some(offset) = next_unused.object_offset().to_usize() {
                            if offset <= old_size {
                                debug!("Using unused address {:?}..{:?}", unused, next_unused);
                                return Some(unused);
                            }
                        }
                    }
                }
            }
            let new_shmem_size = usize::max(object_size.to_usize()?, old_size * 2);
            let new_shmem_id = self.alloc_shmem(new_shmem_size)?;
            let result = SharedAddress::new(new_shmem_id, object_size, ObjectOffset(0));
            let new_unused = Some(SharedAddress::new(
                new_shmem_id,
                object_size,
                ObjectOffset::from_u64(object_size.to_u64()?)?,
            ));
            let next_unused = unused.and_then(|unused| unused.checked_add(object_size));
            if next_unused
                == atomic_unused.compare_and_swap(next_unused, new_unused, Ordering::SeqCst)
            {
                debug!("Using fresh shem address {:?}", result);
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
        if data == 0 {
            None
        } else {
            Some(unsafe { mem::transmute(data) })
        }
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
    fn checked_add(self, size: ObjectSize) -> Option<SharedAddress> {
        let address = self.to_u64()?;
        let size = size.to_u64()?;
        address.checked_add(size).and_then(SharedAddress::from_u64)
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
        1u64.checked_shl(self.0 as u32)
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

// A marker trait for types that it's safe to take a reference to in shared memory
// This is more restrictive than `SharedMemoryCast`, for example even though `usize` is
// `SharedMemoryCat` it's not safe to make a `&usize` pointing into shared memory,
// because if another process writes to that address, then that causes UB
// (the rust compiler is allowed to assume that the value pointed to by a `&usize`
// doesn't change during the references lifetime).
//
// I don't think there's a safe mutable version of this, since it would then be
// possible to create `&mut T` which alias. For example if `Box<AtomicUSize>`
// implemented this, and an honest agent put two such boxes into shared memory,
// and an attacker overwrote the address of the second by the address of the first,
// then an honest agent could create two `&mut AtomicUsize` that alias, which is
// insta-UB.
//
// `SharedMemRef` is to `SharedMemCast` as `Sync` is to `Send`.

unsafe trait SharedMemRef {}

unsafe impl SharedMemRef for AtomicUsize {}
unsafe impl<T> SharedMemRef for AtomicPtr<T> {}
// etc.

unsafe impl SharedMemRef for () {}
unsafe impl<T1, T2> SharedMemRef for (T1, T2)
where
    T1: SharedMemRef,
    T2: SharedMemRef,
{
}
unsafe impl<T1> SharedMemRef for (T1,) where T1: SharedMemRef {}
unsafe impl<T1, T2, T3> SharedMemRef for (T1, T2, T3)
where
    T1: SharedMemRef,
    T2: SharedMemRef,
    T3: SharedMemRef,
{
}
// etc

pub struct SharedBox<T> {
    address: SharedAddress,
    marker: PhantomData<T>,
}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedBox<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedBox<T> {}
unsafe impl<T: Sync> Sync for SharedBox<T> {}
unsafe impl<T: Send> Send for SharedBox<T> {}

impl<T> SharedBox<T> {
    // This is unsafe because if you create in one allocator and read from another
    // then you can get UB.
    pub unsafe fn new_in(data: T, alloc: &ShmemAllocator) -> Option<SharedBox<T>> {
        let size = mem::size_of::<T>();
        let address = unsafe { alloc.alloc_bytes(size)? };
        let ptr = alloc.get_bytes(address)? as *mut T;
        unsafe { ptr.write_volatile(data) };
        let marker = PhantomData;
        Some(SharedBox { address, marker })
    }

    // If you create in one allocator and read from another
    // then you can get an invalid pointer.
    pub fn as_ptr_in(&self, alloc: &ShmemAllocator) -> *mut T {
        alloc.get_bytes(self.address).unwrap_or(ptr::null_mut()) as *mut T
    }

    pub fn try_new(data: T) -> Option<SharedBox<T>> {
        unsafe { SharedBox::new_in(data, &ALLOCATOR) }
    }

    pub fn new(data: T) -> SharedBox<T> {
        SharedBox::try_new(data).expect("Failed to allocate shared box")
    }

    pub fn as_ptr(&self) -> *mut T {
        self.as_ptr_in(&ALLOCATOR)
    }

    pub fn address(&self) -> SharedAddress {
        self.address
    }
}

impl<T: SharedMemRef> Deref for SharedBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.as_ptr() }
    }
}

impl<T> Drop for SharedBox<T> {
    fn drop(&mut self) {
        unsafe {
            self.as_ptr().read();
            ALLOCATOR.free_bytes(self.address);
        }
    }
}

pub struct SharedVec<T> {
    address: SharedAddress,
    length: AtomicUsize,
    marker: PhantomData<T>,
}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedVec<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedVec<T> {}
unsafe impl<T: Sync> Sync for SharedVec<T> {}
unsafe impl<T: Send> Send for SharedVec<T> {}

impl<T> SharedVec<T> {
    // This is unsafe because if you create in one allocator and read from another
    // then you can get UB.
    pub unsafe fn from_iter_in<C>(collection: C, alloc: &ShmemAllocator) -> Option<SharedVec<T>>
    where
        C: IntoIterator<Item = T>,
        C::IntoIter: ExactSizeIterator,
    {
        let iter = collection.into_iter();
        let length = iter.len();
        debug!("Allocating vector of length {}", length);
        let size = mem::size_of::<T>() * length;
        let address = unsafe { alloc.alloc_bytes(size)? };
        let ptr = alloc.get_bytes(address)? as *mut T;
        debug!("Initializing vector");
        for (index, item) in iter.enumerate() {
            ptr.offset(index as isize).write_volatile(item);
        }
        let length = AtomicUsize::new(length);
        let marker = PhantomData;
        Some(SharedVec {
            address,
            length,
            marker,
        })
    }

    // If you create in one allocator and read from another
    // then you can get an invalid pointer.
    pub fn as_ptr_in(&self, alloc: &ShmemAllocator) -> *mut T {
        alloc.get_bytes(self.address).unwrap_or(ptr::null_mut()) as *mut T
    }

    pub fn try_from_iter<C>(collection: C) -> Option<SharedVec<T>>
    where
        C: IntoIterator<Item = T>,
        C::IntoIter: ExactSizeIterator,
    {
        unsafe { SharedVec::from_iter_in(collection, &ALLOCATOR) }
    }

    pub fn from_iter<C>(collection: C) -> SharedVec<T>
    where
        C: IntoIterator<Item = T>,
        C::IntoIter: ExactSizeIterator,
    {
        SharedVec::try_from_iter(collection).expect("Failed to allocate shared vec")
    }

    pub fn as_ptr(&self) -> *mut T {
        self.as_ptr_in(&ALLOCATOR)
    }

    pub fn address(&self) -> SharedAddress {
        self.address
    }

    pub fn len(&self) -> usize {
        self.length.load(Ordering::Relaxed)
    }
}

impl<T: SharedMemRef> Deref for SharedVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl<T> Drop for SharedVec<T> {
    fn drop(&mut self) {
        // TODO
    }
}

#[test]
fn test_one_box() {
    let boxed: SharedBox<AtomicUsize> = SharedBox::new(AtomicUsize::new(37));
    let val = boxed.load(Ordering::SeqCst);
    assert_eq!(val, 37);
}

#[test]
fn test_five_boxes() {
    let boxed: [SharedBox<AtomicUsize>; 5] = [
        SharedBox::new(AtomicUsize::new(1)),
        SharedBox::new(AtomicUsize::new(2)),
        SharedBox::new(AtomicUsize::new(3)),
        SharedBox::new(AtomicUsize::new(4)),
        SharedBox::new(AtomicUsize::new(5)),
    ];
    for i in 0..5 {
        let val = boxed[i].load(Ordering::SeqCst);
        assert_eq!(val, i + 1);
    }
}

#[test]
fn test_vector() {
    let vec = SharedVec::from_iter((0..37).map(|i| AtomicUsize::new(i + 1)));
    let mut last = 0;
    for (i, atomic) in vec.iter().enumerate() {
        let val = atomic.load(Ordering::SeqCst);
        assert_eq!(val, i + 1);
        assert_eq!(last, i);
        last = val;
    }
    assert_eq!(last, 37);
}
