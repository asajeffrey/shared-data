/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use arrayvec::ArrayString;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

#[derive(Clone, Eq, Debug, PartialEq)]
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
