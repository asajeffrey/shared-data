/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use shared_memory::SharedMemCast;

#[derive(Clone, Copy, Default, Eq, Debug, Ord, PartialEq, PartialOrd)]
pub struct ObjectSize(pub(crate) u8);

impl ToPrimitive for ObjectSize {
    fn to_u64(&self) -> Option<u64> {
        1u64.checked_shl(self.0 as u32)
    }

    fn to_i64(&self) -> Option<i64> {
        self.to_u64().as_ref().and_then(ToPrimitive::to_i64)
    }
}

impl FromPrimitive for ObjectSize {
    fn from_u64(data: u64) -> Option<ObjectSize> {
        if data.is_power_of_two() {
            Some(ObjectSize(63 - data.leading_zeros() as u8))
        } else {
            None
        }
    }

    fn from_i64(data: i64) -> Option<ObjectSize> {
        u64::from_i64(data).and_then(ObjectSize::from_u64)
    }
}

impl ObjectSize {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn ceil(size: usize) -> ObjectSize {
        ObjectSize(64 - (size - 1).leading_zeros() as u8)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn floor(size: usize) -> ObjectSize {
        ObjectSize(63 - size.leading_zeros() as u8)
    }
}

unsafe impl SharedMemCast for ObjectSize {}
