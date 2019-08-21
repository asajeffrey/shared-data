use sharing_is_caring::ALLOCATOR;
use std::process::Command;
use std::env;

mod harness;

#[test]
fn test_setup() {
    harness::setup();
}