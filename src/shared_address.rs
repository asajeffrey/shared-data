/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::ObjectOffset;
use crate::ObjectSize;
use crate::SharedAddressRange;
use crate::ShmemId;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use shared_memory::SharedMemCast;
use std::mem;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

// Using repr C implies that on big-endian architectures
// we can use atomic addition on an address in the last field to mean atomic
// addition on the offset (possibly overflowing into the padding).
#[cfg(target_endian = "big")]
#[repr(C)]
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddress {
    shmem_id: ShmemId,
    shmem_size: ObjectSize,
    padding: u8,
    object_offset: ObjectOffset,
}

// Ditto for the first field on little-endian.
#[cfg(target_endian = "little")]
#[repr(C)]
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddress {
    object_offset: ObjectOffset,
    padding: u8,
    shmem_size: ObjectSize,
    shmem_id: ShmemId,
}

impl From<u64> for SharedAddress {
    fn from(data: u64) -> SharedAddress {
        unsafe { mem::transmute(data) }
    }
}

impl From<SharedAddress> for u64 {
    fn from(address: SharedAddress) -> u64 {
        unsafe { mem::transmute(address) }
    }
}

impl SharedAddress {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn new(
        shmem_id: ShmemId,
        shmem_size: ObjectSize,
        object_offset: ObjectOffset,
    ) -> SharedAddress {
        SharedAddress {
            shmem_id,
            shmem_size,
            object_offset,
            padding: 0,
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn shmem_id(self) -> ShmemId {
        self.shmem_id
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn shmem_size(&self) -> ObjectSize {
        self.shmem_size
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn object_offset(&self) -> ObjectOffset {
        self.object_offset
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn checked_add(&self, size: ObjectSize) -> Option<SharedAddressRange> {
        let end = ObjectSize::ceil(
            self.object_offset
                .to_usize()?
                .checked_add(size.to_usize()?)?,
        );
        if end <= self.shmem_size {
            Some(SharedAddressRange::new(
                self.shmem_id,
                self.shmem_size,
                self.object_offset,
                size,
            ))
        } else {
            None
        }
    }
}

unsafe impl SharedMemCast for SharedAddress {}
