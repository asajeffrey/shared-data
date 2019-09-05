/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::SharedOption;
use crate::SharedRc;
use crate::SharedVec;
use crate::Volatile;
use shared_memory::SharedMemCast;
use std::ops::Deref;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

pub(crate) struct SharedChannel<T: SharedMemCast> {
    buffer: SharedVec<SharedOption<T>>,
    start: AtomicUsize,
    finish: AtomicUsize,
    // Initially none, but set to be the channel if it grows.
    grown: SharedOption<SharedRc<SharedChannel<T>>>,
    // TODO: condition variable
}

impl<T: SharedMemCast> SharedChannel<T> {
    fn try_new(capacity: usize) -> Option<SharedChannel<T>> {
        Some(SharedChannel {
            buffer: SharedVec::try_from_iter((0..capacity).map(|_| SharedOption::none()))?,
            start: AtomicUsize::new(0),
            finish: AtomicUsize::new(0),
            grown: SharedOption::none(),
        })
    }
}

#[derive(Clone)]
pub struct SharedSender<T: SharedMemCast>(SharedRc<SharedChannel<T>>);

impl<T: SharedMemCast> SharedSender<T> {
    pub fn try_send(&mut self, mut data: T) -> Result<(), T> {
        loop {
            let capacity = self.0.buffer.len();
            if let Some(grown) = self.0.grown.volatile_peek() {
                self.0 = grown.deref().clone();
                continue;
            }
            let index = self.0.finish.fetch_add(1, Ordering::SeqCst);
            if let Err(unsent) = self.0.buffer[index % capacity].put(data) {
                if let Some(grown) = SharedChannel::try_new(capacity * 2) {
                    let _ = self.0.grown.put(SharedRc::new(grown));
                    data = unsent;
                    continue;
                } else {
                    return Err(unsent);
                }
            }
            // TODO: signal the condition variable
            return Ok(());
        }
    }
}

pub struct SharedReceiver<T: SharedMemCast>(SharedRc<SharedChannel<T>>);

impl<T: SharedMemCast> SharedReceiver<T> {
    pub fn try_recv(&mut self) -> Option<T> {
        loop {
            let capacity = self.0.buffer.len();
            let index = self.0.start.fetch_add(1, Ordering::SeqCst);
            if let Some(result) = self.0.buffer[index % capacity].take() {
                if capacity <= index {
                    // We overflowed, but the buffer is circular, so we just mod
                    self.0.start.fetch_sub(capacity, Ordering::SeqCst);
                    self.0.finish.fetch_sub(capacity, Ordering::SeqCst);
                }
                return Some(result);
            }
            if let Some(grown) = self.0.grown.volatile_peek() {
                if index == self.0.finish.load(Ordering::SeqCst) {
                    self.0 = grown.deref().clone();
                    continue;
                }
            }
            self.0.start.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
    }

    pub fn try_peek(&self) -> Option<&Volatile<T>> {
        let capacity = self.0.buffer.len();
        let index = self.0.start.load(Ordering::SeqCst) % capacity;
        self.0.buffer[index].volatile_peek()
    }
}

pub fn channel<T: SharedMemCast>() -> Option<(SharedSender<T>, SharedReceiver<T>)> {
    let channel = SharedRc::try_new(SharedChannel::try_new(1)?)?;
    Some((SharedSender(channel.clone()), SharedReceiver(channel)))
}
