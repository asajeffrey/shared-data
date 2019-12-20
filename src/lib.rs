/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]

mod allocator;
mod atomic_shared_address;
mod atomic_shared_address_range;
mod object_offset;
mod object_size;
mod shared_address;
mod shared_address_range;
mod shared_box;
mod shared_channel;
mod shared_option;
mod shared_rc;
mod shared_vec;
mod shmem_id;
mod shmem_name;

// All unsafe code lives here
mod unsafe_code;

// Reexport traits.
pub use shared_memory::SharedMemCast;

pub use allocator::get_bootstrap_name;
pub use allocator::set_bootstrap_name;
pub use shared_address_range::SharedAddressRange;
pub use shared_box::SharedBox;
pub use shared_channel::channel;
pub use shared_channel::SharedReceiver;
pub use shared_channel::SharedSender;
pub use shared_option::SharedOption;
pub use shared_rc::SharedRc;
pub use shared_vec::SharedVec;
pub use unsafe_code::SharedMemRef;
pub use unsafe_code::Volatile;

// Should these be publicly exported
pub(crate) use allocator::ShmemAllocator;
pub(crate) use allocator::ALLOCATOR;
pub(crate) use atomic_shared_address::AtomicSharedAddress;
pub(crate) use atomic_shared_address_range::AtomicSharedAddressRange;
pub(crate) use object_offset::ObjectOffset;
pub(crate) use object_size::ObjectSize;
pub(crate) use shared_address::SharedAddress;
pub(crate) use shmem_id::ShmemId;
pub(crate) use shmem_name::ShmemName;
pub(crate) use unsafe_code::SyncSharedMem;
