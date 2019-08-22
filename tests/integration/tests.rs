use crate::harness::spawn_child;
use experiments::ALLOCATOR;
use experiments::SharedAddress;
use experiments::SharedBox;
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use num_traits::ToPrimitive;
use std::env;
use std::process::Command;

// An enum of all the tests
#[derive(Copy, Clone, Debug, FromPrimitive, ToPrimitive)]
pub enum ChildId {
    Fail,
    Noop,
}

impl ChildId {
    pub fn run(&self, address: SharedAddress) {
        match self {
            ChildId::Fail => run_fail(address),
            ChildId::Noop => run_noop(address),
        }
    }
}

// A child process that does nothing
fn run_noop(_address: SharedAddress) {}

// A child process that fails
fn run_fail(_address: SharedAddress) {
    assert_eq!(1, 2);
}

#[test]
fn test_setup() {
    let mut child = spawn_child(ChildId::Noop, SharedAddress::default());
    assert!(child.wait().unwrap().success());
}

#[test]
fn test_setup_failure() {
    let mut child = spawn_child(ChildId::Fail, SharedAddress::default());
    assert!(!child.wait().unwrap().success());
}
