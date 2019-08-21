use sharing_is_caring::ALLOCATOR;
use std::env;
use std::process::Command;

mod harness;

#[test]
fn test_setup() {
    harness::setup();
}
