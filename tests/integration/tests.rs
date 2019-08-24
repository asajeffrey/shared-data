use crate::harness::spawn_child;
use experiments::SharedAddress;
use experiments::SharedBox;
use experiments::ALLOCATOR;
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
    SharedBox,
}

impl ChildId {
    pub fn run(&self, address: SharedAddress) {
        match self {
            ChildId::Fail => run_fail(address),
            ChildId::Noop => run_noop(address),
            ChildId::SharedBox => run_shared_box(address),
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
fn test_setup_success() {
    let boxed: SharedBox<usize> = SharedBox::new(37);
    let mut child = spawn_child(ChildId::Noop, boxed.address());
    assert!(child.wait().unwrap().success());
}

#[test]
fn test_setup_failure() {
    let boxed: SharedBox<usize> = SharedBox::new(37);
    let mut child = spawn_child(ChildId::Fail, boxed.address());
    assert!(!child.wait().unwrap().success());
}

#[test]
fn test_shared_box() {
    let boxed: SharedBox<usize> = SharedBox::new(37);
    let mut child = spawn_child(ChildId::SharedBox, boxed.address());
    assert!(child.wait().unwrap().success());
    let val = unsafe { boxed.as_ptr().read_volatile() };
    assert_eq!(val, 37);
}

fn run_shared_box(address: SharedAddress) {
    //let boxed: SharedBox<usize> = SharedBox::new(37);
    //let ptr = boxed.as_ptr().unwrap().as_ptr();
    //let val = unsafe { ptr.read_volatile() };
    //assert_eq!(val, 37);
}
