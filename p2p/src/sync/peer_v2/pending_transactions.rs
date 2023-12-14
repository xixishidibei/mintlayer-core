// Copyright (c) 2021-2023 RBB S.r.l
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

use std::{cmp::Reverse, collections::BinaryHeap};
use tokio::time::Instant;

use common::{chain::Transaction, primitives::Id};

pub struct PendingTransactions {
    txs: BinaryHeap<Reverse<(Instant, Id<Transaction>)>>,
}

impl PendingTransactions {
    pub fn new() -> Self {
        Self {
            txs: Default::default(),
        }
    }

    pub fn push(&mut self, tx: Id<Transaction>, due_time: Instant) {
        self.txs.push(Reverse((due_time, tx)));
    }

    pub fn pop(&mut self) -> Option<Id<Transaction>> {
        self.txs.pop().map(|item| {
            let (_, tx) = item.0;
            tx
        })
    }

    pub async fn due(&self) {
        match self.txs.peek() {
            Some(item) => {
                let (due, _) = item.0;
                if Instant::now() < due {
                    tokio::time::sleep_until(due).await;
                }
            }
            None => std::future::pending().await,
        }
    }
}
