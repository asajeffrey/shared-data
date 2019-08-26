/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::ObjectOffset;
use crate::ObjectSize;
use crate::ShmemId;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;
use shared_memory::SharedMemCast;
use std::mem;

#[cfg(target_endian = "big")]
#[repr(C)]
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddress {
    shmem_id: ShmemId,
    object_size: ObjectSize,
    padding: u8,
    object_offset: ObjectOffset,
}

#[cfg(target_endian = "little")]
#[repr(C)]
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddress {
    object_offset: ObjectOffset,
    padding: u8,
    object_size: ObjectSize,
    shmem_id: ShmemId,
}

impl FromPrimitive for SharedAddress {
    fn from_u64(data: u64) -> Option<SharedAddress> {
        if data == 0 {
            None
        } else {
            Some(unsafe { mem::transmute(data) })
        }
    }

    fn from_i64(data: i64) -> Option<SharedAddress> {
        u64::from_i64(data).and_then(SharedAddress::from_u64)
    }
}

impl ToPrimitive for SharedAddress {
    fn to_u64(&self) -> Option<u64> {
        Some(unsafe { mem::transmute(*self) })
    }

    fn to_i64(&self) -> Option<i64> {
        self.to_u64().as_ref().and_then(ToPrimitive::to_i64)
    }
}

impl SharedAddress {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn new(shmem_id: ShmemId, size: ObjectSize, offset: ObjectOffset) -> SharedAddress {
        SharedAddress {
            shmem_id: shmem_id,
            object_size: size,
            padding: 0,
            object_offset: offset,
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn checked_add(self, size: ObjectSize) -> Option<SharedAddress> {
        let address = self.to_u64()?;
        let size = size.to_u64()?;
        address.checked_add(size).and_then(SharedAddress::from_u64)
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
}

unsafe impl SharedMemCast for SharedAddress {}
