/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::ObjectOffset;
use crate::ObjectSize;
use crate::SharedAddress;
use crate::ShmemId;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use shared_memory::SharedMemCast;
use std::mem;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddressRange {
    shmem_id: ShmemId,
    shmem_size: ObjectSize,
    object_offset: ObjectOffset,
    object_size: ObjectSize,
}

impl From<u64> for SharedAddressRange {
    fn from(data: u64) -> SharedAddressRange {
        unsafe { mem::transmute(data) }
    }
}

impl From<SharedAddressRange> for u64 {
    fn from(address: SharedAddressRange) -> u64 {
        unsafe { mem::transmute(address) }
    }
}

impl SharedAddressRange {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn new(
        shmem_id: ShmemId,
        shmem_size: ObjectSize,
        object_offset: ObjectOffset,
        object_size: ObjectSize,
    ) -> SharedAddressRange {
        SharedAddressRange {
            shmem_id,
            shmem_size,
            object_offset,
            object_size,
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn shmem_id(self) -> ShmemId {
        self.shmem_id
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn object_size(&self) -> ObjectSize {
        self.object_size
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn object_offset(&self) -> ObjectOffset {
        self.object_offset
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn object_end(&self) -> Option<ObjectOffset> {
        ObjectOffset::from_u64(
            self.object_offset
                .to_u64()?
                .checked_add(self.object_size.to_u64()?)?,
        )
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn end_address(&self) -> Option<SharedAddress> {
        Some(SharedAddress::new(
            self.shmem_id,
            self.shmem_size,
            self.object_end()?,
        ))
    }
}

unsafe impl SharedMemCast for SharedAddressRange {}
