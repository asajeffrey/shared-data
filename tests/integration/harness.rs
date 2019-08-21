use experiments::ALLOCATOR;
use std::env;
use std::process::Child;
use std::process::Command;

// This code is run in the main test process
pub fn spawn_child() -> Child {
    // Get the name of the shared memory
    let shmem_path = ALLOCATOR.shmem().get_os_path();

    // The executable for the child process, which does nothing
    // but call back here. Assumes the layout of the target directory a bit.
    let mut exe_path = env::current_exe().unwrap();
    exe_path.pop();
    exe_path.pop();
    exe_path.push("child");

    // Spawn a child process
    Command::new(exe_path).arg(shmem_path).spawn().unwrap()
}

// This code is run in the child processes
pub fn child(shmem_path: String) {
    // Bootstrap the shared memory
    experiments::bootstrap(shmem_path.clone());

    // Double-check that the allocator has been configured
    // with the right path
    assert_eq!(ALLOCATOR.shmem().get_os_path(), shmem_path);
}
