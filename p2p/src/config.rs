// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://spdx.org/licenses/MIT
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use serde::{Deserialize, Serialize};

/// The p2p subsystem configuration.
#[derive(Serialize, Deserialize, Debug)]
pub struct P2pConfig {
    /// Address to bind P2P to.
    pub address: String,
    /// The score threshold after which a peer is baned.
    pub ban_threshold: u32,
    /// The timeout value in seconds.
    pub timeout: u64,
}

impl P2pConfig {
    /// Creates a new p2p configuration instance.
    pub fn new() -> Self {
        Self {
            address: "/ip6/::1/tcp/3031".into(),
            ban_threshold: 100,
            timeout: 10,
        }
    }
}
