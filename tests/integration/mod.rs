use experiments::ALLOCATOR;
use std::env;
use std::process::Command;

mod harness;

#[test]
fn test_setup() {
    let mut child = harness::spawn_child();
    assert!(child.wait().unwrap().success())
}
