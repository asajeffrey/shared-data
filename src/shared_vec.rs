/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::unsafe_code;
use crate::SharedAddressRange;
use crate::SharedMemRef;
use crate::ShmemAllocator;
use crate::Volatile;
use crate::ALLOCATOR;
use log::debug;
use shared_memory::SharedMemCast;
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ptr;
use std::slice;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub struct SharedVec<T: SharedMemCast> {
    address: SharedAddressRange,
    length: AtomicUsize,
    marker: PhantomData<T>,
}

impl<T: SharedMemCast> SharedVec<T> {
    pub fn from_iter_in<C>(collection: C, alloc: &ShmemAllocator) -> Option<SharedVec<T>>
    where
        C: IntoIterator<Item = T>,
        C::IntoIter: ExactSizeIterator,
    {
        let iter = collection.into_iter();
        let length = iter.len();
        debug!("Allocating vector of length {}", length);
        let size = mem::size_of::<T>() * length;
        let address = alloc.alloc_bytes(size)?;
        let bytes = alloc.get_bytes(address)?;
        let slice = Volatile::<T>::slice_from_volatile_bytes(bytes, length)?;
        debug!("Initializing vector");
        for (item, volatile) in iter.zip(slice) {
            volatile.write_volatile(item);
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
        alloc
            .get_bytes(self.address)
            .map(|bytes| bytes.as_ptr() as *mut T)
            .unwrap_or(ptr::null_mut())
    }

    pub fn try_from_iter<C>(collection: C) -> Option<SharedVec<T>>
    where
        C: IntoIterator<Item = T>,
        C::IntoIter: ExactSizeIterator,
    {
        SharedVec::from_iter_in(collection, &ALLOCATOR)
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

    pub fn get_in<'a>(&'a self, alloc: &'a ShmemAllocator) -> Option<&'a [Volatile<T>]> {
        let bytes = alloc.get_bytes(self.address)?;
        let length = self.length.load(Ordering::SeqCst);
        Volatile::slice_from_volatile_bytes(bytes, length)
    }

    pub fn try_get(&self) -> Option<&[Volatile<T>]> {
        self.get_in(&ALLOCATOR)
    }

    pub fn address(&self) -> SharedAddressRange {
        self.address
    }

    pub fn len(&self) -> usize {
        self.length.load(Ordering::Relaxed)
    }
}

impl<T: SharedMemCast + SharedMemRef> Deref for SharedVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        if let Some(volatile) = self.try_get() {
            unsafe_code::slice_from_volatile(volatile)
        } else {
            unsafe_code::slice_empty()
        }
    }
}

impl<T: SharedMemCast> Drop for SharedVec<T> {
    fn drop(&mut self) {
        // TODO
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
