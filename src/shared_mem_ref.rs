/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;

// A marker trait for types that it's safe to take a reference to in shared memory
// This is more restrictive than `SharedMemoryCast`, for example even though `usize` is
// `SharedMemoryCat` it's not safe to make a `&usize` pointing into shared memory,
// because if another process writes to that address, then that causes UB
// (the rust compiler is allowed to assume that the value pointed to by a `&usize`
// doesn't change during the references lifetime).
//
// I don't think there's a safe mutable version of this, since it would then be
// possible to create `&mut T` which alias. For example if `Box<AtomicUSize>`
// implemented this, and an honest agent put two such boxes into shared memory,
// and an attacker overwrote the address of the second by the address of the first,
// then an honest agent could create two `&mut AtomicUsize` that alias, which is
// insta-UB.
//
// `SharedMemRef` is to `SharedMemCast` as `Sync` is to `Send`.

pub unsafe trait SharedMemRef {}

unsafe impl SharedMemRef for AtomicBool {}
unsafe impl SharedMemRef for AtomicUsize {}
unsafe impl SharedMemRef for AtomicU64 {}
unsafe impl<T> SharedMemRef for AtomicPtr<T> {}
// etc.

unsafe impl SharedMemRef for () {}
unsafe impl<T1, T2> SharedMemRef for (T1, T2)
where
    T1: SharedMemRef,
    T2: SharedMemRef,
{
}
unsafe impl<T1> SharedMemRef for (T1,) where T1: SharedMemRef {}
unsafe impl<T1, T2, T3> SharedMemRef for (T1, T2, T3)
where
    T1: SharedMemRef,
    T2: SharedMemRef,
    T3: SharedMemRef,
{
}
// etc
