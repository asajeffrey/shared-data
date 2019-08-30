/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use arrayvec::ArrayString;
use std::convert::AsRef;
use std::ffi::OsStr;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

#[derive(Clone, Copy, Default, Eq, Debug, PartialEq)]
pub struct ShmemName(ArrayString<[u8; 32]>);

impl ShmemName {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn from_str(name: &str) -> Option<Self> {
        let name = ArrayString::from(name).ok()?;
        Some(ShmemName(name))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<OsStr> for ShmemName {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref().as_ref()
    }
}
