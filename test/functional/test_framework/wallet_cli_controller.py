#!/usr/bin/env python3
#  Copyright (c) 2022-2023 RBB S.r.l
#  Copyright (c) 2014-2021 The Bitcoin Core developers
#  opensource@mintlayer.org
#  SPDX-License-Identifier: MIT
#  Licensed under the MIT License;
#  you may not use this file except in compliance with the License.
#  You may obtain a copy of the License at
#
#  https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.
"""A wrapper around a CLI wallet instance"""

import os
import asyncio
import re
from dataclasses import dataclass
from tempfile import NamedTemporaryFile

from typing import Optional, List, Tuple, Union

ONE_MB = 2**20
READ_TIMEOUT_SEC = 30
DEFAULT_ACCOUNT_INDEX = 0

@dataclass
class UtxoOutpoint:
    id: str
    index: int

    def __str__(self):
        return f'tx({self.id},{self.index})'

@dataclass
class PoolData:
    pool_id: str
    balance: str
    creation_block_height: int
    timestamp: int
    staker: str
    decommission_key: str
    vrf_public_key: str

@dataclass
class DelegationData:
    delegation_id: str
    balance: str

@dataclass
class CreatedBlockInfo:
    block_id: str
    block_height: str

class WalletCliController:

    def __init__(self, node, config, log, wallet_args: List[str] = [], chain_config_args: List[str] = []):
        self.log = log
        self.node = node
        self.config = config
        self.wallet_args = wallet_args
        self.chain_config_args = chain_config_args

    async def __aenter__(self):
        wallet_cli = os.path.join(self.config["environment"]["BUILDDIR"], "test_wallet"+self.config["environment"]["EXEEXT"] )
        cookie_file = os.path.join(self.node.datadir, ".cookie")
        wallet_args = ["regtest", "--rpc-address", self.node.url.split("@")[1], "--rpc-cookie-file", cookie_file] + self.wallet_args + self.chain_config_args
        self.wallet_log_file = NamedTemporaryFile(prefix="wallet_stderr_", dir=os.path.dirname(self.node.datadir), delete=False)
        self.wallet_commands_file = NamedTemporaryFile(prefix="wallet_commands_responses_", dir=os.path.dirname(self.node.datadir), delete=False)

        self.process = await asyncio.create_subprocess_exec(
            wallet_cli, *wallet_args,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=self.wallet_log_file,
        )
        return self

    async def __aexit__(self, exc_type, exc_value, traceback):
        self.log.debug("exiting wallet")
        await self._write_command("exit\n")
        await self.process.communicate()
        self.wallet_log_file.close()
        self.wallet_commands_file.close()

    async def _read_available_output(self) -> str:
        try:
            output = await asyncio.wait_for(self.process.stdout.read(ONE_MB), timeout=READ_TIMEOUT_SEC)
            self.wallet_commands_file.write(output)
            result = output.decode().strip()

            try:
                while True:
                    output = await asyncio.wait_for(self.process.stdout.read(ONE_MB), timeout=0.1)
                    if not output:
                        break
                    self.wallet_commands_file.write(output)
                    result += output.decode().strip()
            except:
                pass

            return result
        except:
            self.wallet_commands_file.write(b"read from stdout timedout\n")
            return ''

    async def _write_command(self, cmd: str) -> str:
        encoded_cmd = cmd.encode()
        self.wallet_commands_file.write(b"writing command: ")
        self.wallet_commands_file.write(encoded_cmd)
        self.process.stdin.write(encoded_cmd)
        await self.process.stdin.drain()
        return await self._read_available_output()

    async def create_wallet(self, name: str = "wallet") -> str:
        wallet_file = os.path.join(self.node.datadir, name)
        return await self._write_command(f"wallet-create {wallet_file} store-seed-phrase\n")

    async def open_wallet(self, name: str) -> str:
        wallet_file = os.path.join(self.node.datadir, name)
        return await self._write_command(f"wallet-open {wallet_file}\n")

    async def recover_wallet(self, mnemonic: str, name: str = "recovered_wallet") -> str:
        wallet_file = os.path.join(self.node.datadir, name)
        return await self._write_command(f"wallet-create {wallet_file} store-seed-phrase \"{mnemonic}\"\n")

    async def close_wallet(self) -> str:
        return await self._write_command("wallet-close\n")

    async def show_seed_phrase(self) -> Optional[str]:
        output = await self._write_command("wallet-show-seed-phrase\n")
        if output.startswith("The stored seed phrase is"):
            mnemonic = output[output.find("\"") + 1:-1]
            return mnemonic
        # wallet doesn't have the seed phrase stored
        return None

    async def encrypt_private_keys(self, password: str) -> str:
        return await self._write_command(f"wallet-encrypt-private-keys {password}\n")

    async def unlock_private_keys(self, password: str) -> str:
        return await self._write_command(f"wallet-unlock-private-keys {password}\n")

    async def lock_private_keys(self) -> str:
        return await self._write_command(f"wallet-lock-private-keys\n")

    async def remove_private_keys_encryption(self) -> str:
        return await self._write_command(f"wallet-disable-private-keys-encryption\n")

    async def get_best_block_height(self) -> str:
        return await self._write_command("node-best-block-height\n")

    async def get_best_block(self) -> str:
        return await self._write_command("node-best-block-id\n")

    async def create_new_account(self, name: Optional[str] = '') -> str:
        return await self._write_command(f"account-create {name}\n")

    async def select_account(self, account_index: int) -> str:
        return await self._write_command(f"account-select {account_index}\n")

    async def set_lookahead_size(self, size: int, force_reduce: bool) -> str:
        i_know_what_i_am_doing = "i-know-what-i-am-doing" if force_reduce else ""
        return await self._write_command(f"wallet-set-lookahead-size {size} {i_know_what_i_am_doing}\n")

    async def new_public_key(self) -> bytes:
        public_key = await self._write_command("address-new-public-key\n")

        # remove the pub key enum value, the first one byte
        pub_key_bytes = bytes.fromhex(public_key)[1:]
        return pub_key_bytes

    async def new_address(self) -> str:
        return await self._write_command(f"address-new\n")

    async def list_utxos(self, utxo_types: str = '', with_locked: str = '', utxo_states: List[str] = []) -> List[UtxoOutpoint]:
        output = await self._write_command(f"account-utxos {utxo_types} {with_locked} {''.join(utxo_states)}\n")

        pattern = r'UtxoOutPoint\s*{[^}]*Id<Transaction>\{0x([^}]*)\}[^}]*index:\s*(\d+)'
        matches = re.findall(pattern, output, re.DOTALL)
        return [UtxoOutpoint(id=match[0].strip(), index=int(match[1].strip())) for match in matches]

    async def get_transaction(self, tx_id: str) -> str:
        return await self._write_command(f"transaction-get {tx_id}\n")

    async def get_raw_signed_transaction(self, tx_id: str) -> str:
        return await self._write_command(f"transaction-get-signed-raw {tx_id}\n")

    async def send_to_address(self, address: str, amount: int, selected_utxos: List[UtxoOutpoint] = []) -> str:
        return await self._write_command(f"address-send {address} {amount} {' '.join(map(str, selected_utxos))}\n")

    async def send_tokens_to_address(self, token_id: str, address: str, amount: Union[float, str]):
        return await self._write_command(f"token-send {token_id} {address} {amount}\n")

    async def issue_new_token(self,
                              token_ticker: str,
                              number_of_decimals: int,
                              metadata_uri: str,
                              destination_address: str,
                              token_supply: str = 'unlimited',
                              is_freezable: str = 'freezable') -> Tuple[Optional[str], Optional[str]]:
        output = await self._write_command(f'token-issue-new "{token_ticker}" "{number_of_decimals}" "{metadata_uri}" {destination_address} {token_supply} {is_freezable}\n')
        if output.startswith("A new token has been issued with ID"):
            return output[output.find(':')+2:], None

        return None, output

    async def mint_tokens(self, token_id: str, address: str, amount: int) -> str:
        return await self._write_command(f"token-mint {token_id} {address} {amount}\n")

    async def unmint_tokens(self, token_id: str, amount: int) -> str:
        return await self._write_command(f"token-unmint {token_id} {amount}\n")

    async def lock_token_supply(self, token_id: str) -> str:
        return await self._write_command(f"token-lock-supply {token_id}\n")

    async def freeze_token(self, token_id: str, is_unfreezable: str) -> str:
        return await self._write_command(f"token-freeze {token_id} {is_unfreezable}\n")

    async def unfreeze_token(self, token_id: str) -> str:
        return await self._write_command(f"token-unfreeze {token_id}\n")

    async def change_token_authority(self, token_id: str, new_authority: str) -> str:
        return await self._write_command(f"token-change-authority {token_id} {new_authority}\n")

    async def issue_new_nft(self,
                            destination_address: str,
                            media_hash: str,
                            name: str,
                            description: str,
                            ticker: str,
                            creator: Optional[str] = '',
                            icon_uri: Optional[str] = '',
                            media_uri: Optional[str] = '',
                            additional_metadata_uri: Optional[str] = ''):
        output = await self._write_command(f"token-nft-issue-new {destination_address} {media_hash} {name} {description} {ticker} {creator} {icon_uri} {media_uri} {additional_metadata_uri}\n")
        if output.startswith("A new NFT has been issued with ID"):
            return output[output.find(':')+2:]

        self.log.error(f"err: {output}")
        return None

    async def create_stake_pool(self,
                                amount: int,
                                cost_per_block: int,
                                margin_ratio_per_thousand: float,
                                decommission_addr: Optional[str] = '') -> str:
        return await self._write_command(f"staking-create-pool {amount} {cost_per_block} {margin_ratio_per_thousand} {decommission_addr}\n")

    async def decommission_stake_pool(self, pool_id: str) -> str:
        return await self._write_command(f"staking-decommission-pool {pool_id}\n")

    async def decommission_stake_pool_request(self, pool_id: str) -> str:
        return await self._write_command(f"staking-decommission-pool-request {pool_id}\n")

    async def sign_raw_transaction(self, transaction: str) -> str:
        return await self._write_command(f"account-sign-raw-transaction {transaction}\n")

    async def submit_transaction(self, transaction: str) -> str:
        return await self._write_command(f"node-submit-transaction {transaction}\n")

    async def list_pool_ids(self) -> List[PoolData]:
        output = await self._write_command("staking-list-pool-ids\n")
        self.log.info(f"pools: {output}");
        pattern = r"Pool Id: ([a-zA-Z0-9]+), Balance: (\d+), Creation Block heigh: (\d+), timestamp: (\d+), staker ([a-zA-Z0-9]+), decommission_key ([a-zA-Z0-9]+), vrf_public_key ([a-zA-Z0-9]+)"
        matches = re.findall(pattern, output)
        return [PoolData(pool_id, balance, int(height), timestamp, staker, decommission_key, vrf_public_key) for pool_id, balance, height, timestamp, staker, decommission_key, vrf_public_key in matches]

    async def list_created_blocks_ids(self) -> List[CreatedBlockInfo]:
        output =  await self._write_command("staking-list-created-block-ids\n")
        self.log.info(output)
        pattern = r"\((\d+),\s*([0-9a-fA-F]+)\)"
        matches = re.findall(pattern, output)
        return [CreatedBlockInfo(block_id, block_height) for block_height, block_id in matches]

    async def create_delegation(self, address: str, pool_id: str) -> Optional[str]:
        output = await self._write_command(f"delegation-create {address} {pool_id}\n")
        pattern = r'Delegation id: ([a-zA-Z0-9]+)'
        match = re.search(pattern, output)
        if match:
            return match.group(1)
        else:
            return None

    async def stake_delegation(self, amount: int, delegation_id: str) -> str:
        return await self._write_command(f"delegation-stake {amount} {delegation_id}\n")

    async def list_delegation_ids(self) -> List[DelegationData]:
        output = await self._write_command("delegation-list-ids\n")
        pattern = r'Delegation Id: ([a-zA-Z0-9]+), Balance: (\d+)'
        matches = re.findall(pattern, output)
        return [DelegationData(delegation_id, balance) for delegation_id, balance in matches]

    async def deposit_data(self, data: str) -> str:
        return await self._write_command(f"address-deposit-data \"{data}\"\n")

    async def sync(self) -> str:
        return await self._write_command("wallet-sync\n")

    async def rescan(self) -> str:
        return await self._write_command("wallet-rescan\n")

    async def start_staking(self) -> str:
        return await self._write_command(f"staking-start\n")

    async def stop_staking(self) -> str:
        return await self._write_command(f"staking-stop\n")

    async def get_addresses_usage(self) -> str:
        return await self._write_command("address-show\n")

    async def get_vrf_addresses_usage(self) -> str:
        return await self._write_command("staking-show-vrf-public-keys\n")

    async def get_legacy_vrf_public_key(self) -> str:
        return await self._write_command("staking-show-legacy-vrf-key\n")

    async def get_balance(self, with_locked: str = 'unlocked', utxo_states: List[str] = ['confirmed']) -> str:
        return await self._write_command(f"account-balance {with_locked} {' '.join(utxo_states)}\n")

    async def list_pending_transactions(self) -> List[str]:
        output = await self._write_command(f"transaction-list-pending\n")
        pattern = r'id: Id<Transaction>\{0x([^}]*)\}'
        return re.findall(pattern, output)

    async def abandon_transaction(self, tx_id: str) -> str:
        return await self._write_command(f"transaction-abandon {tx_id}\n")
