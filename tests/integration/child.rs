use env_logger;
use std::env;

mod harness;
mod tests;

// A program that calls back into the harness
fn main() {
    env_logger::init();
    let mut args = env::args();
    let _exe = args.next().unwrap();
    let shmem_path = args.next().unwrap();
    let child_name = args.next().unwrap();
    let address_name = args.next().unwrap();
    harness::child(shmem_path, child_name, address_name);
}
