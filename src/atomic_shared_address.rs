/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::ObjectSize;
use crate::SharedAddress;
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
    pub fn compare_and_swap(
        &self,
        current: Option<SharedAddress>,
        new: Option<SharedAddress>,
        order: Ordering,
    ) -> Option<SharedAddress> {
        let current = current
            .as_ref()
            .and_then(SharedAddress::to_u64)
            .unwrap_or(0);
        let new = new.as_ref().and_then(SharedAddress::to_u64).unwrap_or(0);
        let bits = self.0.compare_and_swap(current, new, order);
        SharedAddress::from_u64(bits)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn fetch_add(&self, size: ObjectSize, order: Ordering) -> Option<SharedAddress> {
        let size = size.to_u64()?;
        let bits = self.0.fetch_add(size, order);
        let result = SharedAddress::from_u64(bits);
        if result.is_none() {
            self.0.fetch_sub(size, order);
        }
        result
    }
}

unsafe impl SharedMemCast for AtomicSharedAddress {}
