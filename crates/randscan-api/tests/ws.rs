//! `/ws` over a real socket: the upgrade, the subscribe / ping / unsubscribe protocol, a
//! malformed message, fan-out of a broadcast event, and the connection's removal on close. The
//! client is a few lines of RFC 6455 over `TcpStream` (no WebSocket client crate in the tree).
//! The database pool is lazy and never used, so this needs no `DATABASE_URL`.

use randscan_api::{create_router, mail::MailSender, ratelimit::RateLimiter, ApiConfig, AppState};
use randscan_core::{BlockSummary, BroadcastEvent};
use randscan_db::DbPool;
use randscan_indexer::{Broadcaster, IndexerConfig, IndexerService};
use randscan_ws::WsManager;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Client(TcpStream);

impl Client {
    async fn connect(addr: std::net::SocketAddr) -> Client {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(
            format!(
                "GET /ws HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(s.read_u8().await.unwrap());
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.starts_with("HTTP/1.1 101"), "upgrade refused: {head}");
        assert!(
            head.contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="),
            "accept key for the RFC's sample nonce: {head}"
        );
        Client(s)
    }

    async fn send_frame(&mut self, opcode: u8, payload: &[u8]) {
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut f = vec![0x80 | opcode];
        assert!(payload.len() < 126);
        f.push(0x80 | payload.len() as u8);
        f.extend_from_slice(&mask);
        f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.write_all(&f).await.unwrap();
    }

    async fn send(&mut self, v: Value) {
        self.send_text(&v.to_string()).await
    }

    async fn send_text(&mut self, t: &str) {
        self.send_frame(0x1, t.as_bytes()).await
    }

    /// The next text frame as JSON; `None` if nothing arrives within `wait`.
    async fn next(&mut self, wait: Duration) -> Option<Value> {
        tokio::time::timeout(wait, async {
            loop {
                let b0 = self.0.read_u8().await.unwrap();
                let b1 = self.0.read_u8().await.unwrap();
                let len = match b1 & 0x7f {
                    126 => self.0.read_u16().await.unwrap() as usize,
                    127 => self.0.read_u64().await.unwrap() as usize,
                    n => n as usize,
                };
                let mut payload = vec![0; len];
                self.0.read_exact(&mut payload).await.unwrap();
                if b0 & 0x0f == 0x1 {
                    return serde_json::from_slice(&payload).unwrap();
                }
            }
        })
        .await
        .ok()
    }

    async fn expect(&mut self) -> Value {
        self.next(Duration::from_secs(5)).await.expect("a frame")
    }
}

async fn serve() -> (std::net::SocketAddr, Arc<WsManager>) {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://nobody:nobody@127.0.0.1:1/none")
        .unwrap();
    let db = DbPool::new(pool);
    let manager = Arc::new(WsManager::new());
    let state = AppState {
        db: db.clone(),
        indexer: Arc::new(IndexerService::new(
            IndexerConfig::default(),
            db,
            Broadcaster::new(),
        )),
        ws_manager: manager.clone(),
        config: Arc::new(ApiConfig::default()),
        limiter: Arc::new(RateLimiter::new()),
        mailer: None::<Arc<dyn MailSender>>,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = create_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, manager)
}

fn block(height: i64) -> BlockSummary {
    BlockSummary {
        hash: format!("{height:064x}"),
        height,
        view: height * 2,
        parent: "0".repeat(64),
        proposer: "proposer".into(),
        timestamp_ms: 1_789_000_000_000,
        tx_count: 3,
        justify_view: height * 2 - 1,
    }
}

#[tokio::test]
async fn the_socket_speaks_the_subscription_protocol() {
    let (addr, manager) = serve().await;
    let mut c = Client::connect(addr).await;
    let mut other = Client::connect(addr).await;
    for _ in 0..50 {
        if manager.connection_count().await == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(manager.connection_count().await, 2);

    c.send(json!({ "type": "ping" })).await;
    assert_eq!(c.expect().await, json!({ "type": "pong" }));

    c.send_text("this is not json").await;
    let err = c.expect().await;
    assert_eq!(err["type"], "error");
    assert!(err["message"]
        .as_str()
        .unwrap()
        .starts_with("bad message: "));

    c.send(json!({ "type": "subscribe", "channel": "blocks" }))
        .await;
    let ack = c.expect().await;
    assert_eq!(ack["type"], "subscribed");
    assert_eq!(ack["channel"], "blocks");
    assert_eq!(ack["subscription_id"].as_str().unwrap().len(), 36);

    // A block event reaches the subscriber (a 126+ byte frame: the extended length path) and
    // not the connection that subscribed to nothing.
    manager.broadcast(BroadcastEvent::NewBlock(block(9))).await;
    let ev = c.expect().await;
    assert_eq!(ev["type"], "new_block");
    assert_eq!(ev["block"]["height"], 9);
    assert_eq!(ev["block"]["tx_count"], 3);
    assert!(other.next(Duration::from_millis(200)).await.is_none());

    c.send(json!({ "type": "unsubscribe", "channel": "blocks" }))
        .await;
    assert_eq!(
        c.expect().await,
        json!({ "type": "unsubscribed", "channel": "blocks" })
    );
    manager.broadcast(BroadcastEvent::NewBlock(block(10))).await;
    assert!(c.next(Duration::from_millis(200)).await.is_none());

    // A close frame ends the connection and the manager forgets it; a binary frame is ignored.
    other.send_frame(0x2, b"ignored").await;
    other.send(json!({ "type": "ping" })).await;
    assert_eq!(other.expect().await, json!({ "type": "pong" }));
    c.send_frame(0x8, &[]).await;
    for _ in 0..100 {
        if manager.connection_count().await == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(manager.connection_count().await, 1);
}
