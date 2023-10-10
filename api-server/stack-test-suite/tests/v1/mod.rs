mod block;
mod block_header;
mod block_reward;
mod block_transaction_ids;
mod chain_at_height;
mod chain_tip;
mod transaction;
mod transaction_merkle_path;

use api_server_common::storage::{
    impls::in_memory::transactional::TransactionalApiServerInMemoryStorage,
    storage_api::{ApiServerStorageWrite, ApiServerTransactionRw, Transactional},
};
use blockchain_scanner_lib::{
    blockchain_state::BlockchainState, sync::local_state::LocalBlockchainState,
};
use chainstate_test_framework::TestFramework;
use common::{
    chain::config::create_unit_test_config,
    primitives::{BlockHeight, Idable},
};
use hex::ToHex;
use rstest::rstest;
use serde_json::json;
use std::{net::TcpListener, sync::Arc};
use test_utils::random::{make_seedable_rng, Rng, Seed};
use web_server::{api::web_server, ApiServerWebServerState};

async fn spawn_webserver(url: &str) -> (tokio::task::JoinHandle<()>, reqwest::Response) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let web_server_state = {
            let chain_config = Arc::new(create_unit_test_config());
            let storage = TransactionalApiServerInMemoryStorage::new(&chain_config);

            ApiServerWebServerState {
                db: Arc::new(storage),
                chain_config: Arc::clone(&chain_config),
            }
        };

        web_server(listener, web_server_state).await.unwrap();
    });

    // Given that the listener port is open, this will block until a response is made (by the web server, which takes the listener over)
    let response = reqwest::get(format!("http://{}:{}{url}", addr.ip(), addr.port()))
        .await
        .unwrap();

    (task, response)
}

#[tokio::test]
async fn server_status() {
    let (task, response) = spawn_webserver("/").await;

    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), r#"{"versions":["1.0.0"]}"#);

    task.abort();
}

#[tokio::test]
async fn bad_request() {
    let (task, response) = spawn_webserver("/non-existent-url").await;

    assert_eq!(response.status(), 400);
    assert_eq!(response.text().await.unwrap(), r#"{"error":"Bad request"}"#);

    task.abort();
}

#[tokio::test]
async fn v1_chain_genesis() {
    let url = "/api/v1/chain/genesis";

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let (tx, rx) = tokio::sync::oneshot::channel();

    let task = tokio::spawn({
        async move {
            let web_server_state = {
                let chain_config = Arc::new(create_unit_test_config());
                let expected_genesis = chain_config.genesis_block().clone();

                _ = tx.send(expected_genesis);

                let storage = TransactionalApiServerInMemoryStorage::new(&chain_config);

                ApiServerWebServerState {
                    db: Arc::new(storage),
                    chain_config: Arc::clone(&chain_config),
                }
            };

            web_server(listener, web_server_state).await
        }
    });

    // Given that the listener port is open, this will block until a response is made (by the web server, which takes the listener over)
    let response = reqwest::get(format!("http://{}:{}{url}", addr.ip(), addr.port()))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let expected_genesis = rx.await.unwrap();

    let body = response.text().await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert_eq!(
        body["block_id"].as_str().unwrap(),
        expected_genesis.get_id().to_hash().encode_hex::<String>()
    );

    task.abort();
}
