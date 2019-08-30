/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::SharedAddressRange;
use crate::SharedMemRef;
use crate::ShmemAllocator;
use crate::Volatile;
use crate::ALLOCATOR;
use num_traits::ToPrimitive;
use shared_memory::SharedMemCast;
use std::convert::From;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;

/// An owned pointer into shared memory.
pub struct SharedBox<T: SharedMemCast> {
    address: SharedAddressRange,
    marker: PhantomData<T>,
}

impl<T: SharedMemCast> SharedBox<T> {
    pub(crate) fn new_in(data: T, alloc: &ShmemAllocator) -> Option<SharedBox<T>> {
        let size = mem::size_of::<T>();
        let address = alloc.alloc_bytes(size)?;
        let bytes = alloc.get_bytes(address)?;
        let volatile = Volatile::<T>::from_volatile_bytes(bytes)?;
        let marker = PhantomData;
        volatile.write_volatile(data);
        Some(SharedBox { address, marker })
    }

    pub(crate) fn get_in<'a>(&'a self, alloc: &'a ShmemAllocator) -> Option<&'a Volatile<T>> {
        let bytes = alloc.get_bytes(self.address)?;
        Volatile::from_volatile_bytes(bytes)
    }

    /// Allocates a new box in shared memory, returning `None` if allocation failed.
    pub fn try_new(data: T) -> Option<SharedBox<T>> {
        SharedBox::new_in(data, &ALLOCATOR)
    }

    /// Allocates a new box in shared memory, panicing if allocation failed.
    pub fn new(data: T) -> SharedBox<T> {
        SharedBox::try_new(data).expect("Failed to allocate shared box")
    }

    /// Accesses a box in shared memory, returning `None` if the box refers to inaccessible memory.
    pub fn try_get(&self) -> Option<&Volatile<T>> {
        self.get_in(&ALLOCATOR)
    }

    /// Accesses a box in shared memory, panicing if the box refers to inaccessible memory.
    pub fn get(&self) -> &Volatile<T> {
        self.try_get().expect("Failed to deref shared box")
    }

    /// The shared address of the box.
    pub fn address(&self) -> SharedAddressRange {
        self.address
    }

    /// Create a box from a shared address.
    pub(crate) fn unchecked_from_address(address: SharedAddressRange) -> SharedBox<T> {
        SharedBox {
            address,
            marker: PhantomData,
        }
    }
}

impl<T: SharedMemCast> TryFrom<SharedAddressRange> for SharedBox<T> {
    type Error = ();
    fn try_from(address: SharedAddressRange) -> Result<SharedBox<T>, ()> {
        if mem::size_of::<T>() <= address.object_size().to_usize().ok_or(())? {
            Ok(SharedBox::unchecked_from_address(address))
        } else {
            Err(())
        }
    }
}

impl<T: SharedMemCast> From<SharedBox<T>> for SharedAddressRange {
    fn from(boxed: SharedBox<T>) -> SharedAddressRange {
        let address = boxed.address;
        mem::forget(boxed);
        address
    }
}

impl<T: SharedMemCast + SharedMemRef> Deref for SharedBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.get().deref()
    }
}

impl<T: SharedMemCast> Drop for SharedBox<T> {
    fn drop(&mut self) {
        // TODO: make it possible to use drop_in_place
        if let Some(volatile) = self.try_get() {
            volatile.read_volatile();
        }
        ALLOCATOR.free_bytes(self.address);
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
