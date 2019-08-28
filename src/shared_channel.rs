/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::AtomicSharedAddress;
use crate::SharedAddress;
use crate::SharedBox;
use crate::SharedRc;
use crate::SharedVec;
use crate::SharedMemRef;
use shared_memory::SharedMemCast;
use std::iter;
use std::mem;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

// TODO: grow buffers
const BUFFER_SIZE: usize = 256;

// TODO: support enums
const UNOCCUPIED: u8 = 0;
const RESERVED: u8 = 1;
const OCCUPIED: u8 = 2;

struct SharedChannel<T> {
    // TODO: once MaybeUninit has stabalized, use it
    buffer: SharedVec<T>,
    occupied: SharedVec<AtomicU8>,
    start: AtomicIsize,
    finish: AtomicIsize,
    grown: AtomicShared<SharedRc<SharedChannel<T>>>,
    // TODO: grow the channel
    // TODO: condition variable
}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedChannel<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedChannel<T> {}

impl<T: SharedMemCast> SharedChannel<T> {
    fn new() -> SharedChannel<T> {
        SharedChannel {
            buffer: SharedVec::from_iter((0..BUFFER_SIZE).map(|_| mem::uninitialized())),
            occupied: SharedVec::from_iter((0..BUFFER_SIZE).map(|_| AtomicBool::new(false))),
            start: AtomicIsize::new(0),
            finish: AtomicIsize::new(0),
            grown: AtomicShared::null(),
	}
    }
}

pub struct SharedSender<T> (SharedRc<SharedChannel<T>>);

unsafe impl<T: SharedMemCast> SharedMemCast for SharedSender<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedSender<T> {}

impl<T: SharedMemCast> SharedSender<T> {
    pub fn send(&mut self, data: T) {
        loop {
	    if let Some(grown) = self.0.grown.load(Ordering::SeqCst) {
	        self.0 = grown;
	        continue;
	    }
            let index = self.0.finish.fetch_add(1, Ordering::SeqCst) % BUFFER_SIZE;
            if index == (BUFFER_SIZE - 1) {
                // We overflowed, but the buffer is circular, so we just mod
                self.0.finish.fetch_sub(BUFFER_SIZE, Ordering::SeqCst);
            }
	    if UNOCCUPIED != self.occupied[index].compare_and_swap(UNOCCUPIED, RESERVED, Ordering::SeqCst) {
	        let grown = SharedRc::new(SharedChannel::new());
		self.0.grown.compare_and_swap(None, Some(grown), Ordering::SeqCst);
		continue;
            }
	    unsafe { self.0.buffer.as_ptr().offset(index).write_volatile(data) };
	    self.occupied[index].store(OCCUPIED, Ordering::SeqCst);
	    // TODO: signal the condition variable
	    return;
	}
    }
}

pub struct SharedReceiver<T> (SharedRc<SharedChannel<T>>);

unsafe impl<T: SharedMemCast> SharedMemCast for SharedReceiver<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedReceiver<T> {}

impl<T: SharedMemCast> SharedReceiver<T> {
    pub fn try_recv(&self) -> Option<T> {
        let index = self.0.start.fetch_add(1, Ordering::SeqCst) % BUFFER_SIZE;
        if index == (BUFFER_SIZE - 1) {
            // We overflowed, but the buffer is circular, so we just mod
            self.0.start.fetch_sub(BUFFER_SIZE, Ordering::SeqCst);
        }
        if OCCUPIED != self.occupied[index].compare_and_swap(OCCUPIED, RESERVED, Ordering::SeqCst) {
	    return None;
        }
	let result = unsafe { self.0.buffer.as_ptr().offset(index).read_volatile() };
        self.occupied[index].store(UNOCCUPIED, Ordering::SeqCst);
	Some(result)
    }

    pub fn try_peek(&self) -> Option<&T> {
        let index = self.0.start.load(Ordering::SeqCst) % BUFFER_SIZE;
        if OCCUPIED != self.occupied[index].load(Ordering::SeqCst) {
	    return None;
        }
	unsafe { &*self.0.buffer.as_ptr().offset(index) }
    }
}

