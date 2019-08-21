use sharing_is_caring::ALLOCATOR;
use std::process::Command;

pub fn setup() {
    let shmem_path = ALLOCATOR.shmem().get_os_path();
}
