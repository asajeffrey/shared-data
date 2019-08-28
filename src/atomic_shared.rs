/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::ObjectSize;
use crate::SharedAddress;
use crate::SharedAddressRange;
use crate::SharedMemRef;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use shared_memory::SharedMemCast;
use std::convert::From;
use std::convert::Into;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

#[derive(Default)]
pub struct AtomicShared<T>(AtomicU64, PhantomData<T>);

impl<T> AtomicShared<T> {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn load(&self, order: Ordering) -> T
    where
        T: From<SharedAddressRange>,
    {
        T::from(self.0.load(order).into())
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn try_load(&self, order: Ordering) -> Option<T>
    where
        T: TryFrom<SharedAddressRange>,
    {
        T::try_from(self.0.load(order).into()).ok()
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn try_compare_and_swap(&self, current: T, new: T, order: Ordering) -> Option<T>
    where
        T: TryFrom<SharedAddressRange>,
        SharedAddressRange: From<T>,
    {
        let current = u64::from(SharedAddressRange::from(current));
        let new = u64::from(SharedAddressRange::from(new));
        let result = self.0.compare_and_swap(current, new, order);
        T::try_from(result.into()).ok()
    }
}

unsafe impl<T: SharedMemCast> SharedMemCast for AtomicShared<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for AtomicShared<T> {}
