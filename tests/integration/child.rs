use std::env;

mod harness;

// A program that calls back into the harness
fn main() {
    let shmem_path = env::args().skip(1).next().unwrap();
    harness::child(shmem_path);
}
