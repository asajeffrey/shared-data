/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::Volatile;
use shared_memory::SharedMemCast;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

// TODO: support enums
const UNOCCUPIED: u8 = 0;
const RESERVED: u8 = 1;
const OCCUPIED: u8 = 2;

/// Optional shared data
pub struct SharedOption<T: SharedMemCast> {
    data: Volatile<T>,
    occupied: AtomicU8,
}

impl<T: SharedMemCast> SharedOption<T> {
    pub fn none() -> SharedOption<T> {
        SharedOption {
            data: Volatile::zeroed(),
            occupied: AtomicU8::new(UNOCCUPIED),
        }
    }

    pub fn some(value: T) -> SharedOption<T> {
        SharedOption {
            data: Volatile::new(value),
            occupied: AtomicU8::new(OCCUPIED),
        }
    }

    pub fn volatile_peek(&self) -> Option<&Volatile<T>> {
        if self.occupied.load(Ordering::SeqCst) == OCCUPIED {
            Some(&self.data)
        } else {
            None
        }
    }

    pub fn put(&self, value: T) -> Result<(), T> {
        if self
            .occupied
            .compare_and_swap(UNOCCUPIED, RESERVED, Ordering::SeqCst)
            == UNOCCUPIED
        {
            self.data.write_volatile(value);
            self.occupied.store(OCCUPIED, Ordering::SeqCst);
            Ok(())
        } else {
            Err(value)
        }
    }

    pub fn take(&self) -> Option<T> {
        if self
            .occupied
            .compare_and_swap(OCCUPIED, RESERVED, Ordering::SeqCst)
            == OCCUPIED
        {
            let result = self.data.read_volatile();
            self.occupied.store(UNOCCUPIED, Ordering::SeqCst);
            Some(result)
        } else {
            None
        }
    }
}
