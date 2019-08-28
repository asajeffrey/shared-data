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

struct SharedChannel<T> {
    // TODO: once MaybeUninit has stabalized, use it
    buffer: SharedVec<T>,
    start: AtomicIsize,
    finish: AtomicIsize,
    size: AtomicUsize,
    grown: AtomicSharedAddress,
    // TODO: grow the channel
    // TODO: condition variable
}

unsafe impl<T: SharedMemCast> SharedMemCast for SharedChannel<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedChannel<T> {}

impl<T: SharedMemCast> SharedChannel<T> {
    fn new() -> SharedChannel<T> {
        SharedChannel {
            buffer: SharedVec::from_iter((0..BUFFER_SIZE).map(|_| mem::uninitialized())),
            start: AtomicIsize::new(1),
            finish: AtomicIsize::new(1),
            size: AtomicUsize::new(0),
            grown: AtomicSharedAddress::new(SharedAddress::null()),
	}
    }
}

pub struct SharedSender<T> (SharedRc<SharedChannel<T>>);

unsafe impl<T: SharedMemCast> SharedMemCast for SharedSender<T> {}
unsafe impl<T: SharedMemCast> SharedMemRef for SharedSender<T> {}

impl<T: SharedMemCast> SharedSender<T> {
    pub fn send(&mut self, data: T) {
        loop {
	    let grown = self.0.grown.load(Ordering::SeqCst);
	    if !grown.is_null() {
	        self.0 = SharedRc::from_address(grown);
		unsafe { self.0.atomic_ref_count().fetch_add(1, Ordering::SeqCst) };
	        continue;
	    }
	    let size = self.0.size.fetch_add(1, Ordering::SeqCst);
            if size >= BUFFER_SIZE {
	        let grown = SharedRc::new(SharedChannel::new());
		if self.0.grown.compare_and_swap(SharedAddress::null(), grown.address(), Ordering::SeqCst).is_null() {
		    self.0 = grown;
		}
		continue;
            }
            let index = self.0.finish.fetch_add(1, Ordering::SeqCst) % BUFFER_SIZE;
            if index == 0 {
                // We overflowed, but the buffer is circular, so we just mod
                self.0.finish.fetch_sub(BUFFER_SIZE, Ordering::SeqCst);
            }
	    unsafe { self.0.buffer.as_ptr().offset(index).write_volatile(data) }; 
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
        let size = self.0.size.fetch_sub(1, Ordering::SeqCst);
	if size <= 0 {
	    self.0.size.fetch_add(1, Ordering::SeqCst);
	    return None;
	}
        let index = self.0.start.fetch_add(1, Ordering::SeqCst) % BUFFER_SIZE;
        if index == 0 {
            // We overflowed, but the buffer is circular, so we just mod
            self.0.start.fetch_sub(BUFFER_SIZE, Ordering::SeqCst);
        }
	Some(unsafe { self.0.buffer.as_ptr().offset(index).read_volatile() })
    }
}

