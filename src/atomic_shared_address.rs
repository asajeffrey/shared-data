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
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

#[derive(Default)]
pub struct AtomicSharedAddress(AtomicU64);

impl AtomicSharedAddress {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn load(&self, order: Ordering) -> SharedAddress {
        SharedAddress::from(self.0.load(order))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn compare_and_swap(
        &self,
        current: SharedAddress,
        new: SharedAddress,
        order: Ordering,
    ) -> SharedAddress {
        let current = u64::from(current);
        let new = u64::from(new);
        let result = self.0.compare_and_swap(current, new, order);
        SharedAddress::from(result)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn fetch_add(&self, size: ObjectSize, order: Ordering) -> Option<SharedAddressRange> {
        let address = SharedAddress::from(self.0.fetch_add(size.to_u64()?, order));
        let result = address.checked_add(size);
        if result.is_none() {
            self.0.fetch_sub(size.to_u64()?, order);
        }
        result
    }
}

unsafe impl SharedMemCast for AtomicSharedAddress {}
unsafe impl SharedMemRef for AtomicSharedAddress {}
