/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::SharedAddressRange;
use crate::SharedBox;
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
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub struct SharedRc<T> {
    address: SharedAddressRange,
    marker: PhantomData<T>,
}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedRc<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedRc<T> {}

impl<T> SharedRc<T> {
    fn from_address(address: SharedAddressRange) -> Self {
        let marker = PhantomData;
        SharedRc { address, marker }
    }

    fn from_box(boxed: SharedBox<(T, AtomicUsize)>) -> Self {
        let address = boxed.address();
        mem::forget(boxed);
        SharedRc::from_address(address)
    }

    fn as_box(&self) -> &SharedBox<(T, AtomicUsize)> {
        unsafe { mem::transmute(self) }
    }

    fn into_box(self) -> SharedBox<(T, AtomicUsize)> {
        unsafe { mem::transmute(self) }
    }

    fn ref_count(&self) -> &AtomicUsize {
        unsafe { &self.as_box().unchecked_deref().1 }
    }

    pub fn try_new(data: T) -> Option<SharedRc<T>> {
        let ref_count = AtomicUsize::new(1);
        Some(SharedRc::from_box(SharedBox::try_new((data, ref_count))?))
    }

    pub fn new(data: T) -> SharedRc<T> {
        SharedRc::try_new(data).expect("Failed to allocate shared Rc")
    }

    pub fn address(&self) -> SharedAddressRange {
        self.address
    }
}

impl<T> TryFrom<SharedAddressRange> for SharedRc<T> {
    type Error = ();
    fn try_from(address: SharedAddressRange) -> Result<SharedRc<T>, ()> {
        if mem::size_of::<(AtomicUsize, T)>() <= address.object_size().to_usize().ok_or(())? {
            let result: SharedRc<T> = SharedRc {
                address,
                marker: PhantomData,
            };
            result.ref_count().fetch_add(1, Ordering::SeqCst);
            Ok(result)
        } else {
            Err(())
        }
    }
}

impl<T> From<SharedRc<T>> for SharedAddressRange {
    fn from(rc: SharedRc<T>) -> SharedAddressRange {
        let address = rc.address;
        mem::forget(rc);
        address
    }
}

impl<T: SharedMemRef> Deref for SharedRc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.as_box().0
    }
}

impl<T> Clone for SharedRc<T> {
    fn clone(&self) -> Self {
        self.ref_count().fetch_add(1, Ordering::SeqCst);
        SharedRc::from_address(self.address)
    }
}

impl<T> Drop for SharedRc<T> {
    fn drop(&mut self) {
        let ref_count = self.ref_count().fetch_sub(1, Ordering::SeqCst);
        if ref_count == 1 {
            self.clone().into_box();
        }
    }
}

#[test]
fn test_rc() {
    let rc: SharedRc<AtomicUsize> = SharedRc::new(AtomicUsize::new(37));
    let val = rc.load(Ordering::SeqCst);
    assert_eq!(val, 37);
}
