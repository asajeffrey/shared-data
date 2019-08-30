/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![allow(unsafe_code)]

use owning_ref::StableAddress;
use shared_memory::SharedMem;
use shared_memory::SharedMemCast;
use std::cell::UnsafeCell;
use std::mem;
use std::ops::Deref;
use std::ptr;
use std::slice;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;

use crate::allocator::ShmemMetadata;
use crate::shared_rc::SharedRcContents;
use crate::AtomicSharedAddress;
use crate::ObjectOffset;
use crate::ObjectSize;
use crate::SharedAddress;
use crate::SharedAddressRange;
use crate::SharedBox;
use crate::SharedRc;
use crate::SharedVec;
use crate::ShmemId;
use crate::ShmemName;

/// A marker trait for types that it's safe to take a reference to in shared memory.
///
/// This is more restrictive than `SharedMemoryCast`, for example even though `usize` is
/// `SharedMemoryCast` it's not safe to make a `&usize` pointing into shared memory,
/// because if another process writes to that address, then that causes UB
/// (the rust compiler is allowed to assume that the value pointed to by a `&usize`
/// doesn't change during the references lifetime).
///
/// I don't think there's a safe mutable version of this, since it would then be
/// possible to create `&mut T` which alias. For example if `Box<AtomicUSize>`
/// implemented this, and an honest agent put two such boxes into shared memory,
/// and an attacker overwrote the address of the second by the address of the first,
/// then an honest agent could create two `&mut AtomicUsize` that alias, which is
/// insta-UB.
///
/// `SharedMemRef` is to `SharedMemCast` as `Sync` is to `Send`.
pub unsafe trait SharedMemRef {}

unsafe impl SharedMemRef for AtomicBool {}
unsafe impl SharedMemRef for AtomicUsize {}
unsafe impl SharedMemRef for AtomicU64 {}
unsafe impl<T> SharedMemRef for AtomicPtr<T> {}
// etc.

unsafe impl SharedMemRef for () {}
unsafe impl<T1> SharedMemRef for (T1,) where T1: SharedMemRef {}
unsafe impl<T1, T2> SharedMemRef for (T1, T2)
where
    T1: SharedMemRef,
    T2: SharedMemRef,
{
}
unsafe impl<T1, T2, T3> SharedMemRef for (T1, T2, T3)
where
    T1: SharedMemRef,
    T2: SharedMemRef,
    T3: SharedMemRef,
{
}
// etc

unsafe impl SharedMemRef for AtomicSharedAddress {}
unsafe impl SharedMemRef for ShmemMetadata {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedBox<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedRc<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedRcContents<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedVec<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for Volatile<T> {}

// Implementations of `SharedMemCast` for types in this crate
unsafe impl SharedMemCast for AtomicSharedAddress {}
unsafe impl SharedMemCast for ObjectOffset {}
unsafe impl SharedMemCast for ObjectSize {}
unsafe impl SharedMemCast for SharedAddress {}
unsafe impl SharedMemCast for ShmemId {}
unsafe impl SharedMemCast for ShmemMetadata {}
unsafe impl SharedMemCast for ShmemName {}
unsafe impl<T: SharedMemCast> SharedMemCast for SharedBox<T> {}
unsafe impl<T: SharedMemCast> SharedMemCast for SharedRc<T> {}
unsafe impl<T: SharedMemCast> SharedMemCast for SharedRcContents<T> {}
unsafe impl<T: SharedMemCast> SharedMemCast for SharedVec<T> {}
unsafe impl<T: SharedMemCast> SharedMemCast for Volatile<T> {}

// `Volatile` is `Send` and `Sync`.
unsafe impl<T: SharedMemCast> Sync for Volatile<T> {}
unsafe impl<T: SharedMemCast> Send for Volatile<T> {}

/// A wrapper round the `SharedMem` type which implements `Sync`.
pub struct SyncSharedMem(*mut Volatile<u8>, usize, SharedMem);

impl SyncSharedMem {
    /// Create a new `SyncSharedMem` from a `SharedMem`.
    pub fn from_shmem(shmem: SharedMem) -> SyncSharedMem {
        let ptr = shmem.get_ptr() as *mut Volatile<u8>;
        let size = shmem.get_size();
        let result = SyncSharedMem(ptr, size, shmem);
        result
    }
}

impl Deref for SyncSharedMem {
    type Target = [Volatile<u8>];

    fn deref(&self) -> &[Volatile<u8>] {
        unsafe { slice::from_raw_parts(self.0, self.1) }
    }
}

unsafe impl Sync for SyncSharedMem {}
unsafe impl StableAddress for SyncSharedMem {}

/// Data stored in memory that can be changed
/// at any time, for example shared memory.
///
/// It is safe to take a reference to a `Volatile<T>` in shared memory,
/// even when it is not safe to take a reference to a `T` in shared memory.
/// For example if a `usize` is stored in shared memory, it is not safe to take
/// a `&usize` reference to it, since the Rust compiler assumes that the value pointed
/// to is immutable, and it's UB if the value changes.
///
/// The trait `SharedMemRef` allows a `&Volatile<T>` to be dereferenced as a `&T` when
/// its safe to do so, for example for `AtomicUsize`.
pub struct Volatile<T>(UnsafeCell<T>);

impl<T: SharedMemCast> Volatile<T> {
    /// Create a new volatile.
    pub fn new(value: T) -> Volatile<T> {
        Volatile(UnsafeCell::new(value))
    }

    /// Try to create a volatile from some volatile bytes.
    /// Returns `None` if there are not enough bytes.
    pub fn from_volatile_bytes(bytes: &[Volatile<u8>]) -> Option<&Volatile<T>> {
        unsafe {
            if mem::size_of::<T>() <= bytes.len() {
                (bytes.as_ptr() as *const Volatile<T>).as_ref()
            } else {
                None
            }
        }
    }

    /// Try to create a slice of volatiles from some volatile bytes.
    /// Returns `None` if there are not enough bytes.
    pub fn slice_from_volatile_bytes(
        bytes: &[Volatile<u8>],
        length: usize,
    ) -> Option<&[Volatile<T>]> {
        unsafe {
            if mem::size_of::<T>() * length <= bytes.len() {
                let ptr = bytes.as_ptr() as *const Volatile<T>;
                Some(slice::from_raw_parts(ptr, length))
            } else {
                None
            }
        }
    }

    /// The volatile as a raw pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.0.get()
    }

    /// Performs a volatile read of the underlying value without moving it. This leaves the memory unchanged.
    ///
    /// This has the same semantics as `ptr::read_volatile`.
    ///
    /// This may have unexpected results if `T` does not implement `Copy`,
    pub fn read_volatile(&self) -> T {
        unsafe { self.as_ptr().read_volatile() }
    }

    /// Performs a volatile write of the underlying value with the given value without reading or dropping the old value.
    ///
    /// This has the same semantics as `ptr::write_volatile`.
    ///
    /// This may have unexpected results if `T` has drop code.
    pub fn write_volatile(&self, value: T) {
        unsafe {
            self.as_ptr().write_volatile(value);
        }
    }
}

impl<T: SharedMemCast + SharedMemRef> Deref for Volatile<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.as_ptr() }
    }
}

/// Convert a slice of volatile values that implement `SharedMemRef`
/// into a slice of values.
pub fn slice_from_volatile<T: SharedMemCast + SharedMemRef>(slice: &[Volatile<T>]) -> &[T] {
    unsafe { mem::transmute(slice) }
}

// Is there a way to do this with safe code?
pub(crate) fn slice_empty<'a, T: 'a>() -> &'a [T] {
    unsafe { slice::from_raw_parts(ptr::NonNull::dangling().as_ptr(), 0) }
}

// Various conversions between `u64` and shared addresses.

impl From<u64> for SharedAddress {
    fn from(data: u64) -> SharedAddress {
        unsafe { mem::transmute(data) }
    }
}

impl From<SharedAddress> for u64 {
    fn from(address: SharedAddress) -> u64 {
        unsafe { mem::transmute(address) }
    }
}

impl From<u64> for SharedAddressRange {
    fn from(data: u64) -> SharedAddressRange {
        unsafe { mem::transmute(data) }
    }
}

impl From<SharedAddressRange> for u64 {
    fn from(address: SharedAddressRange) -> u64 {
        unsafe { mem::transmute(address) }
    }
}
