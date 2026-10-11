//! `RpcClient` against a loopback server that answers one canned HTTP response per connection:
//! what it does with a JSON-RPC error, a null result, an HTTP error status, a body that is not
//! JSON, a node that lacks a method, and a registry paged in more than one request. Also the
//! broadcaster and `IndexerConfig::from_env`. No database, no network beyond loopback.

use randscan_core::{BlockSummary, BroadcastEvent};
use randscan_indexer::{Broadcaster, IndexerConfig, RpcClient, RpcFailure};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Puts the named environment variables back as they were when it is dropped, on a panic too,
/// so a failing assertion cannot leave the process environment altered for the other tests.
struct EnvGuard(Vec<(&'static str, Option<String>)>);

impl EnvGuard {
    fn new(keys: &[&'static str]) -> Self {
        EnvGuard(keys.iter().map(|k| (*k, std::env::var(k).ok())).collect())
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.0.drain(..) {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

/// Serve `answer(request_json)` as `(status, body)` for every request, recording each request.
async fn serve(
    answer: impl Fn(&Value) -> (u16, String) + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = listener.accept().await.unwrap();
            let (log, answer) = (log.clone(), answer.clone());
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let body_at = loop {
                    let n = sock.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buf[..body_at]).to_ascii_lowercase();
                let len: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .map(|v| v.trim().parse().unwrap())
                    .unwrap_or(0);
                while buf.len() < body_at + len {
                    let n = sock.read(&mut chunk).await.unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                }
                let req: Value = serde_json::from_slice(&buf[body_at..body_at + len]).unwrap();
                let (status, body) = answer(&req);
                log.lock().unwrap().push(req);
                let resp = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    (url, seen)
}

fn ok(result: Value) -> (u16, String) {
    (
        200,
        json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string(),
    )
}

fn rpc_error(code: i64, message: &str) -> (u16, String) {
    (
        200,
        json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": code, "message": message } })
            .to_string(),
    )
}

#[tokio::test]
async fn a_result_is_decoded_and_the_request_is_a_jsonrpc_call() {
    let (url, seen) = serve(|_| ok(json!(14))).await;
    let client = RpcClient::new(&url);
    assert_eq!(client.url(), url);
    assert_eq!(client.chain_id().await.unwrap(), 14);
    let sent = seen.lock().unwrap()[0].clone();
    assert_eq!(sent["jsonrpc"], "2.0");
    assert_eq!(sent["method"], "rand_chainId");
    assert_eq!(sent["params"], json!([]));
}

#[tokio::test]
async fn a_node_error_is_a_typed_failure_with_its_code() {
    let (url, _) = serve(|_| rpc_error(-32000, "boom")).await;
    let err = RpcClient::new(&url).head().await.unwrap_err();
    let f = err.downcast_ref::<RpcFailure>().expect("an RpcFailure");
    assert_eq!((f.method.as_str(), f.code), ("rand_getHead", -32000));
    assert_eq!(f.message, "boom");
    assert!(err
        .to_string()
        .contains("rand_getHead failed: boom (-32000)"));
}

#[tokio::test]
async fn a_missing_result_is_an_error_where_one_is_required_and_none_where_it_is_not() {
    let (url, _) = serve(|_| ok(Value::Null)).await;
    let client = RpcClient::new(&url);
    let err = client.chain_id().await.unwrap_err();
    assert!(
        err.to_string().contains("rand_chainId returned null"),
        "{err}"
    );
    assert!(client.block_by_height(5).await.unwrap().is_none());
    assert!(client
        .block_by_hash(&"ab".repeat(32))
        .await
        .unwrap()
        .is_none());
    assert!(
        !client.is_connected().await,
        "a head that is null is not a connection"
    );
}

#[tokio::test]
async fn a_method_the_node_lacks_is_none_but_any_other_failure_is_not() {
    let (url, _) = serve(|req| match req["method"].as_str().unwrap() {
        "rand_getAdmitted" => rpc_error(-32601, "unknown method rand_getAdmitted"),
        "rand_getVersion" => rpc_error(-32000, "internal"),
        _ => ok(Value::Null),
    })
    .await;
    let client = RpcClient::new(&url);
    assert!(client.admitted().await.unwrap().is_none());
    assert!(client.version().await.is_err());
    // An old node without the token registry has an empty one.
    let (old, _) = serve(|_| rpc_error(-32601, "unknown method")).await;
    let list = RpcClient::new(&old).tokens_all().await.unwrap();
    assert!(!list.enabled && list.tokens.is_empty());
    assert!(RpcClient::new(&old).token("1").await.unwrap().is_none());
    assert!(RpcClient::new(&old)
        .token_supply("1")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn http_errors_and_garbage_bodies_are_errors_not_panics() {
    let (url, _) = serve(|_| (503, "unavailable".into())).await;
    let client = RpcClient::new(&url);
    let err = client.chain_id().await.unwrap_err();
    assert!(err.to_string().contains("503"), "{err}");
    assert!(!client.is_connected().await);

    let (url, _) = serve(|_| (200, "<html>not json</html>".into())).await;
    assert!(RpcClient::new(&url).chain_id().await.is_err());

    // A result of the wrong shape fails to decode.
    let (url, _) = serve(|_| ok(json!("not a number"))).await;
    assert!(RpcClient::new(&url).chain_id().await.is_err());

    // Nothing listening at all.
    let dead = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = dead.local_addr().unwrap();
    drop(dead);
    assert!(RpcClient::new(&format!("http://{addr}"))
        .chain_id()
        .await
        .is_err());
}

#[tokio::test]
async fn the_token_registry_is_read_until_a_short_page() {
    let row = |i: u64| {
        json!({
            "index": i, "id": format!("{i:064x}"), "id_text": format!("rpl1x{i}"),
            "name": "T", "symbol": "T", "decimals": 2,
            "authority": { "kind": "none" }, "mint_nonce": 0,
            "total_supply": "0", "registered_at": 1
        })
    };
    // A page is 1000 rows: the first request gets a full one, the second a short one.
    let (url, seen) = serve(move |req| {
        let from = req["params"][0].as_u64().unwrap();
        let rows: Vec<Value> = if from == 0 {
            (0..1000).map(row).collect()
        } else {
            (1000..1003).map(row).collect()
        };
        ok(json!({ "enabled": true, "registration_fee": 7, "next_index": 1003, "tokens": rows }))
    })
    .await;
    let list = RpcClient::new(&url).tokens_all().await.unwrap();
    assert_eq!(list.tokens.len(), 1003);
    assert!(list.enabled);
    assert_eq!(list.next_index, Some(1003));
    let froms: Vec<u64> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|r| r["params"][0].as_u64().unwrap())
        .collect();
    assert_eq!(froms, vec![0, 1000]);
}

fn block(height: i64) -> BlockSummary {
    BlockSummary {
        hash: format!("{height:064x}"),
        height,
        view: 0,
        parent: String::new(),
        proposer: "p".into(),
        timestamp_ms: 0,
        tx_count: 0,
        justify_view: 0,
    }
}

#[tokio::test]
async fn the_broadcaster_reaches_every_subscriber_and_never_fails_without_one() {
    let b = Broadcaster::default();
    b.block(block(1)); // nobody is listening: not an error
    let mut a = b.subscribe();
    let mut c = b.clone().subscribe();
    b.block(block(2));
    for rx in [&mut a, &mut c] {
        match rx.recv().await.unwrap() {
            BroadcastEvent::NewBlock(blk) => assert_eq!(blk.height, 2),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn the_indexer_config_reads_the_environment() {
    let keys = [
        "RPC_URL",
        "POLL_INTERVAL_MS",
        "BATCH_SIZE",
        "STATS_INTERVAL_SECS",
        "NODES_INTERVAL_SECS",
    ];
    let _env = EnvGuard::new(&keys);
    for k in keys {
        std::env::remove_var(k);
    }
    let d = IndexerConfig::from_env();
    assert_eq!(d.rpc_url, "http://127.0.0.1:8545");
    assert_eq!(d.poll_interval.as_millis(), 1000);
    assert_eq!(d.batch_size, 200);
    assert_eq!(d.stats_interval.as_secs(), 5);
    assert_eq!(d.nodes_interval.as_secs(), 60);

    std::env::set_var("RPC_URL", "http://node:9");
    std::env::set_var("POLL_INTERVAL_MS", "250");
    std::env::set_var("BATCH_SIZE", "17");
    std::env::set_var("STATS_INTERVAL_SECS", "not a number");
    std::env::set_var("NODES_INTERVAL_SECS", "9");
    let c = IndexerConfig::from_env();
    assert_eq!(c.rpc_url, "http://node:9");
    assert_eq!(c.poll_interval.as_millis(), 250);
    assert_eq!(c.batch_size, 17);
    assert_eq!(c.stats_interval.as_secs(), 5, "unparsable falls back");
    assert_eq!(c.nodes_interval.as_secs(), 9);
}
