#![feature(allocator_api)]

use context_allocator::memory_sources::MemorySource;

use std::alloc::AllocErr;
use std::num::NonZeroUsize;
use std::ptr::NonNull;

#[derive(Debug)]
pub struct SharedMemorySource {
}

impl MemorySource for SharedMemorySource {
    fn obtain(&self, size: NonZeroUsize) -> Result<NonNull<u8>, AllocErr> {
        unimplemented!();
    }

    fn release(&self, size: NonZeroUsize, ptr: NonNull<u8>) {
        unimplemented!();
    }
}
