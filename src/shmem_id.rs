/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use num_derive::FromPrimitive;
use num_derive::ToPrimitive;

#[derive(Clone, Copy, Default, Eq, Debug, PartialEq, FromPrimitive, ToPrimitive)]
pub struct ShmemId(u16);
