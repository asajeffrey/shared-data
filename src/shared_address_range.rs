/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::ObjectOffset;
use crate::ObjectSize;
use crate::ShmemId;
use num_traits::FromPrimitive;
use num_traits::ToPrimitive;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

/// A range of addresses in shared memory, packed into 64 bits.
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub struct SharedAddressRange {
    shmem_id: ShmemId,
    shmem_size: ObjectSize,
    object_offset: ObjectOffset,
    object_size: ObjectSize,
}

impl SharedAddressRange {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn new(
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
    pub fn null() -> SharedAddressRange {
        SharedAddressRange::from(0)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn shmem_id(self) -> ShmemId {
        self.shmem_id
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn object_size(&self) -> ObjectSize {
        self.object_size
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn object_offset(&self) -> ObjectOffset {
        self.object_offset
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn object_end(&self) -> Option<ObjectOffset> {
        ObjectOffset::from_u64(
            self.object_offset
                .to_u64()?
                .checked_add(self.object_size.to_u64()?)?,
        )
    }
}
