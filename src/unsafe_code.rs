/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use owning_ref::StableAddress;
use shared_memory::SharedMem;
use shared_memory::SharedMemCast;
use std::cell::UnsafeCell;
use std::mem;
use std::ops::Deref;
use std::ptr;
use std::slice;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::allocator::ShmemMetadata;
use crate::SharedBox;
use crate::SharedMemRef;
use crate::SharedRc;
use crate::SharedVec;
use crate::ShmemName;

unsafe impl SharedMemCast for ShmemMetadata {}
unsafe impl SharedMemRef for ShmemMetadata {}

unsafe impl<T: SharedMemCast> SharedMemCast for Volatile<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for Volatile<T> {}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedBox<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedBox<T> {}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedRc<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedRc<T> {}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedVec<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedVec<T> {}

unsafe impl SharedMemCast for ShmemName {}

unsafe impl<T: Send> Sync for Volatile<T> {}

pub struct AtomicSharedMem(AtomicPtr<Volatile<u8>>, AtomicUsize);

impl AtomicSharedMem {
    pub fn new() -> AtomicSharedMem {
        AtomicSharedMem(AtomicPtr::new(ptr::null_mut()), AtomicUsize::new(0))
    }

    pub fn from_shmem(shmem: SharedMem) -> AtomicSharedMem {
        let ptr = shmem.get_ptr() as *mut Volatile<u8>;
        let size = shmem.get_size();
        let result = AtomicSharedMem(AtomicPtr::new(ptr), AtomicUsize::new(size));
        mem::forget(shmem);
        result
    }

    pub fn init(&self, shmem: SharedMem) -> Option<SharedMem> {
        let ptr = shmem.get_ptr() as *mut Volatile<u8>;
        if self
            .0
            .compare_and_swap(ptr::null_mut(), ptr, Ordering::SeqCst)
            .is_null()
        {
            self.1.store(shmem.get_size(), Ordering::SeqCst);
            mem::forget(shmem);
            None
        } else {
            Some(shmem)
        }
    }

    pub fn as_slice(&self) -> Option<&[Volatile<u8>]> {
        unsafe {
            let len = self.1.load(Ordering::SeqCst);
            let ptr = self.0.load(Ordering::SeqCst);
            if len == 0 {
                None
            } else {
                Some(slice::from_raw_parts(ptr, len))
            }
        }
    }
}

pub struct SyncSharedMem(*mut Volatile<u8>, usize, SharedMem);

impl SyncSharedMem {
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

pub struct Volatile<T>(UnsafeCell<T>);

impl<T: SharedMemCast> Volatile<T> {
    pub fn new(value: T) -> Volatile<T> {
        Volatile(UnsafeCell::new(value))
    }

    pub fn from_volatile_bytes(bytes: &[Volatile<u8>]) -> Option<&Volatile<T>> {
        unsafe {
            if mem::size_of::<T>() <= bytes.len() {
                (bytes.as_ptr() as *const Volatile<T>).as_ref()
            } else {
                None
            }
        }
    }

    pub fn as_ptr(&self) -> *mut T {
        self.0.get()
    }

    pub fn get(&self) -> T
    where
        T: Copy,
    {
        unsafe { self.as_ptr().read_volatile() }
    }

    pub fn set(&self, value: T) {
        unsafe {
            self.as_ptr().write_volatile(value);
        }
    }

    pub fn swap(&self, value: T) -> T {
        unsafe {
            let result = self.as_ptr().read_volatile();
            self.as_ptr().write_volatile(value);
            result
        }
    }
}

impl<T: SharedMemCast + SharedMemRef> Deref for Volatile<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.as_ptr() }
    }
}
