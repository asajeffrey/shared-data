mod harness;
mod tests;

// A program that calls back into the harness
#[cfg(not(test))]
fn main() {
    let _ = env_logger::init();
    let mut args = std::env::args();
    let _exe = args.next().unwrap();
    let shmem_path = args.next().unwrap();
    let child_name = args.next().unwrap();
    let address_name = args.next().unwrap();
    harness::child(shmem_path, child_name, address_name);
}
