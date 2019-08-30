#[cfg(test)]
use crate::harness::spawn_child;
#[cfg(not(test))]
use shared_data::SharedAddressRange;
#[cfg(test)]
use shared_data::SharedBox;
#[cfg(test)]
use shared_data::SharedVec;
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
#[cfg(test)]
use std::sync::atomic::Ordering;

// An enum of all the tests
#[derive(Copy, Clone, Debug, FromPrimitive, ToPrimitive)]
pub enum ChildId {
    Fail,
    Noop,
    SharedBox,
    SharedVec,
}

// This is run in the child process, not the main test process
#[cfg(not(test))]
impl ChildId {
    pub fn run(&self, address: SharedAddressRange) {
        match self {
            ChildId::Fail => run_fail(address),
            ChildId::Noop => run_noop(address),
            ChildId::SharedBox => run_shared_box(address),
            ChildId::SharedVec => run_shared_vec(address),
        }
    }
}

// A child process that does nothing
#[cfg(not(test))]
fn run_noop(_address: SharedAddressRange) {}

// A child process that fails
#[cfg(not(test))]
fn run_fail(_address: SharedAddressRange) {
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
    let boxed = SharedBox::new(AtomicUsize::new(37));
    let mut child = spawn_child(ChildId::SharedBox, boxed.address());
    assert!(child.wait().unwrap().success());
    let val = boxed.load(Ordering::SeqCst);
    assert_eq!(val, 37);
}

#[cfg(not(test))]
fn run_shared_box(_address: SharedAddressRange) {
    // TODO
}

#[test]
fn test_vector() {
    let vec = SharedVec::from_iter((0..37).map(|i| AtomicUsize::new(i + 1)));
    let mut last = 0;
    for (i, atomic) in vec.iter().enumerate() {
        let val = atomic.load(Ordering::SeqCst);
        assert_eq!(val, i + 1);
        assert_eq!(last, i);
        last = val;
    }
    assert_eq!(last, 37);
}

#[cfg(not(test))]
fn run_shared_vec(_address: SharedAddressRange) {
    // TODO
}
