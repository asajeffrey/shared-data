use crate::tests::ChildId;
use experiments::SharedAddressRange;
use experiments::ALLOCATOR;
#[cfg(not(test))]
use num_traits::FromPrimitive;
#[cfg(test)]
use num_traits::ToPrimitive;
#[cfg(test)]
use std::env;
#[cfg(test)]
use std::process::Child;
#[cfg(test)]
use std::process::Command;

// This code is run in the main test process
#[cfg(test)]
pub fn spawn_child(child_id: ChildId, address: SharedAddressRange) -> Child {
    // Get the name of the shared memory
    let shmem_path = ALLOCATOR.shmem().get_os_path();

    // The executable for the child process, which does nothing
    // but call back here. Assumes the layout of the target directory a bit.
    let mut exe_path = env::current_exe().unwrap();
    exe_path.pop();
    exe_path.pop();
    exe_path.push("child");

    // Convert the child_id and address to strings
    let child_name = child_id.to_usize().unwrap().to_string();
    let address_name = u64::from(address).to_string();

    // Spawn a child process
    Command::new(exe_path)
        .arg(shmem_path)
        .arg(child_name)
        .arg(address_name)
        .spawn()
        .unwrap()
}

// This code is run in the child processes
#[cfg(not(test))]
pub fn child(shmem_path: String, child_name: String, address_name: String) {
    // Bootstrap the shared memory
    experiments::bootstrap(shmem_path.clone());

    // Double-check that the allocator has been configured
    // with the right path
    assert_eq!(ALLOCATOR.shmem().get_os_path(), shmem_path);

    // Parse the child id and address
    let child_id = ChildId::from_usize(child_name.parse().unwrap()).unwrap();
    let address_number: u64 = address_name.parse().unwrap();
    let address = SharedAddressRange::from(address_number);

    // Run the child
    child_id.run(address);
}
