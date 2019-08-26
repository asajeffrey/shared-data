/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::SharedAddress;
use crate::SharedMemRef;
use crate::ShmemAllocator;
use crate::ALLOCATOR;
use shared_memory::SharedMemCast;
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ptr;

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
        let address = alloc.alloc_bytes(size)?;
        let ptr = alloc.get_bytes(address)? as *mut T;
        ptr.write_volatile(data);
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

#[cfg(test)]
use std::sync::atomic::AtomicUsize;
#[cfg(test)]
use std::sync::atomic::Ordering;

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
