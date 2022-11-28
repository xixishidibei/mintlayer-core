// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::net::mock::peer::Role;

use super::{StreamAdapter, StreamKey};

#[derive(Clone)]
pub struct IdentityStreamKey {}

impl StreamKey for IdentityStreamKey {
    fn gen_new() -> Self {
        IdentityStreamKey {}
    }
}

#[derive(Debug)]
pub struct IdentityStreamAdapter {}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> StreamAdapter<T>
    for IdentityStreamAdapter
{
    type Stream = T;

    type StreamKey = IdentityStreamKey;

    async fn handshake(
        _stream_key: &Self::StreamKey,
        base: T,
        _role: Role,
    ) -> crate::Result<Self::Stream> {
        Ok(base)
    }
}
