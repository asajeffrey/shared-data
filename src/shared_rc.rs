/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::SharedAddressRange;
use crate::SharedBox;
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
use std::ptr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub struct SharedRc<T: SharedMemCast>(SharedBox<SharedRcContents<T>>);

// This is repr(C) to ensure that the data is placed at the beginning
#[repr(C)]
pub(crate) struct SharedRcContents<T: SharedMemCast> {
    data: Volatile<T>,
    ref_count: AtomicUsize,
}

impl<T: SharedMemCast> SharedRc<T> {
    pub fn try_new(data: T) -> Option<SharedRc<T>> {
        let ref_count = AtomicUsize::new(1);
        let data = Volatile::new(data);
        let contents = SharedRcContents { ref_count, data };
        Some(SharedRc(SharedBox::new(contents)))
    }

    pub fn new(data: T) -> SharedRc<T> {
        SharedRc::try_new(data).expect("Failed to allocate shared Rc")
    }

    pub fn address(&self) -> SharedAddressRange {
        self.0.address()
    }
}

impl<T: SharedMemCast> TryFrom<SharedAddressRange> for SharedRc<T> {
    type Error = ();
    fn try_from(address: SharedAddressRange) -> Result<SharedRc<T>, ()> {
        Ok(SharedRc(SharedBox::try_from(address)?))
    }
}

impl<T: SharedMemCast> From<SharedRc<T>> for SharedAddressRange {
    fn from(rc: SharedRc<T>) -> SharedAddressRange {
        let address = rc.0.address();
        mem::forget(rc);
        address
    }
}

impl<T: SharedMemCast + SharedMemRef> Deref for SharedRc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0.data.deref()
    }
}

impl<T: SharedMemCast> Clone for SharedRc<T> {
    fn clone(&self) -> Self {
        self.0.ref_count.fetch_add(1, Ordering::SeqCst);
        SharedRc(SharedBox::unchecked_from_address(self.0.address()))
    }
}

impl<T: SharedMemCast> Drop for SharedRc<T> {
    fn drop(&mut self) {
        let ref_count = self.0.ref_count.fetch_sub(1, Ordering::SeqCst);
        if ref_count > 1 {
            self.0 = SharedBox::unchecked_from_address(SharedAddressRange::null())
        }
    }
}

#[test]
fn test_rc() {
    let rc: SharedRc<AtomicUsize> = SharedRc::new(AtomicUsize::new(37));
    let val = rc.load(Ordering::SeqCst);
    assert_eq!(val, 37);
}
