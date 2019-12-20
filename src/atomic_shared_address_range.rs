/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::ObjectSize;
use crate::SharedAddressRange;
use num_traits::ToPrimitive;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

#[derive(Default)]
pub struct AtomicSharedAddressRange(AtomicU64);

impl AtomicSharedAddressRange {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn load(&self, order: Ordering) -> SharedAddressRange {
        SharedAddressRange::from(self.0.load(order))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn store(&self, value: SharedAddressRange, order: Ordering) {
        self.0.store(u64::from(value), order)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn compare_and_swap(
        &self,
        current: SharedAddressRange,
        new: SharedAddressRange,
        order: Ordering,
    ) -> SharedAddressRange {
        let current = u64::from(current);
        let new = u64::from(new);
        let result = self.0.compare_and_swap(current, new, order);
        SharedAddressRange::from(result)
    }
}
