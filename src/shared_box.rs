/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::SharedAddressRange;
use crate::SharedMemRef;
use crate::ShmemAllocator;
use crate::ALLOCATOR;
use num_traits::ToPrimitive;
use shared_memory::SharedMemCast;
use std::convert::From;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ptr;

pub struct SharedBox<T> {
    address: SharedAddressRange,
    marker: PhantomData<T>,
}

impl<T> SharedBox<T> {
    // This is unsafe because if you create in one allocator and read from another
    // then you can get UB.
    pub unsafe fn new_in(data: T, alloc: &ShmemAllocator) -> Option<SharedBox<T>> {
        let size = mem::size_of::<T>();
        let address = alloc.alloc_bytes(size)?;
        let ptr = alloc.get_bytes(address)?.as_ptr() as *mut T;
        ptr.write_volatile(data);
        let marker = PhantomData;
        Some(SharedBox { address, marker })
    }

    // If you create in one allocator and read from another
    // then you can get an invalid pointer.
    pub fn as_ptr_in(&self, alloc: &ShmemAllocator) -> *mut T {
        alloc
            .get_bytes(self.address)
            .map(|bytes| bytes.as_ptr() as *mut T)
            .unwrap_or(ptr::null_mut())
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

    // This is unsafe because T might not implement SharedMemRef.
    pub unsafe fn unchecked_deref(&self) -> &T {
        &*self.as_ptr()
    }

    pub fn address(&self) -> SharedAddressRange {
        self.address
    }
}

impl<T> TryFrom<SharedAddressRange> for SharedBox<T> {
    type Error = ();
    fn try_from(address: SharedAddressRange) -> Result<SharedBox<T>, ()> {
        if mem::size_of::<T>() <= address.object_size().to_usize().ok_or(())? {
            Ok(SharedBox {
                address,
                marker: PhantomData,
            })
        } else {
            Err(())
        }
    }
}

impl<T> From<SharedBox<T>> for SharedAddressRange {
    fn from(boxed: SharedBox<T>) -> SharedAddressRange {
        let address = boxed.address;
        mem::forget(boxed);
        address
    }
}

impl<T: SharedMemRef> Deref for SharedBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.unchecked_deref() }
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
