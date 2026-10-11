use randscan_core::{BroadcastEvent, WsChannel, WsServerMessage};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info};
use uuid::Uuid;

pub type WsSender = mpsc::UnboundedSender<WsServerMessage>;

struct Connection {
    sender: WsSender,
    channels: HashSet<WsChannel>,
}

pub struct WsManager {
    connections: Arc<RwLock<HashMap<String, Connection>>>,
}

impl WsManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, sender: WsSender) -> String {
        let id = Uuid::new_v4().to_string();
        self.connections.write().await.insert(
            id.clone(),
            Connection {
                sender,
                channels: HashSet::new(),
            },
        );
        debug!("ws connection {} registered", id);
        id
    }

    pub async fn unregister(&self, id: &str) {
        self.connections.write().await.remove(id);
        debug!("ws connection {} unregistered", id);
    }

    pub async fn subscribe(&self, connection_id: &str, channel: WsChannel) {
        if let Some(conn) = self.connections.write().await.get_mut(connection_id) {
            conn.channels.insert(channel);
            let _ = conn.sender.send(WsServerMessage::Subscribed {
                channel,
                subscription_id: Uuid::new_v4().to_string(),
            });
        }
    }

    pub async fn unsubscribe(&self, connection_id: &str, channel: WsChannel) {
        if let Some(conn) = self.connections.write().await.get_mut(connection_id) {
            conn.channels.remove(&channel);
            let _ = conn.sender.send(WsServerMessage::Unsubscribed { channel });
        }
    }

    pub async fn broadcast(&self, event: BroadcastEvent) {
        let channel = event.channel();
        let message = event.to_message();
        for conn in self.connections.read().await.values() {
            if conn.channels.contains(&channel) {
                let _ = conn.sender.send(message.clone());
            }
        }
    }

    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    pub async fn start_broadcast_listener(
        self: Arc<Self>,
        mut receiver: broadcast::Receiver<BroadcastEvent>,
    ) {
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => self.broadcast(event).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        debug!("ws broadcast lagged by {}", n)
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("broadcast channel closed");
                        break;
                    }
                }
            }
        });
    }
}

impl Default for WsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use randscan_core::BlockSummary;

    fn block(height: i64) -> BlockSummary {
        BlockSummary {
            hash: format!("hash-{height}"),
            height,
            view: height * 2,
            parent: "parent".into(),
            proposer: "proposer".into(),
            timestamp_ms: 0,
            tx_count: 0,
            justify_view: 0,
        }
    }

    async fn connect(m: &WsManager) -> (String, mpsc::UnboundedReceiver<WsServerMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (m.register(tx).await, rx)
    }

    #[tokio::test]
    async fn a_subscriber_gets_only_the_channels_it_asked_for() {
        let m = WsManager::new();
        let (a, mut rx_a) = connect(&m).await;
        let (_b, mut rx_b) = connect(&m).await;
        assert_eq!(m.connection_count().await, 2);

        m.subscribe(&a, WsChannel::Blocks).await;
        match rx_a.recv().await.unwrap() {
            WsServerMessage::Subscribed {
                channel,
                subscription_id,
            } => {
                assert_eq!(channel, WsChannel::Blocks);
                assert_eq!(subscription_id.len(), 36, "a uuid");
            }
            other => panic!("expected Subscribed, got {other:?}"),
        }

        m.broadcast(BroadcastEvent::NewBlock(block(7))).await;
        match rx_a.recv().await.unwrap() {
            WsServerMessage::NewBlock { block } => assert_eq!(block.height, 7),
            other => panic!("expected NewBlock, got {other:?}"),
        }
        assert!(
            rx_b.try_recv().is_err(),
            "an unsubscribed connection hears nothing"
        );
    }

    #[tokio::test]
    async fn unsubscribing_stops_delivery_and_is_acknowledged() {
        let m = WsManager::new();
        let (a, mut rx) = connect(&m).await;
        m.subscribe(&a, WsChannel::Blocks).await;
        m.subscribe(&a, WsChannel::Stats).await;
        let _ = (rx.recv().await, rx.recv().await);

        m.unsubscribe(&a, WsChannel::Blocks).await;
        assert!(matches!(
            rx.recv().await.unwrap(),
            WsServerMessage::Unsubscribed {
                channel: WsChannel::Blocks
            }
        ));
        m.broadcast(BroadcastEvent::NewBlock(block(1))).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn an_unknown_connection_is_ignored_and_a_closed_one_forgotten() {
        let m = WsManager::default();
        // Neither call may panic or create state.
        m.subscribe("ghost", WsChannel::Blocks).await;
        m.unsubscribe("ghost", WsChannel::Blocks).await;
        assert_eq!(m.connection_count().await, 0);

        let (a, rx) = connect(&m).await;
        m.subscribe(&a, WsChannel::Blocks).await;
        drop(rx); // the receiving half is gone: a broadcast must not fail
        m.broadcast(BroadcastEvent::NewBlock(block(1))).await;
        m.unregister(&a).await;
        assert_eq!(m.connection_count().await, 0);
    }

    #[tokio::test]
    async fn the_listener_forwards_broadcasts_until_the_channel_closes() {
        let m = Arc::new(WsManager::new());
        let (a, mut rx) = connect(&m).await;
        m.subscribe(&a, WsChannel::Blocks).await;
        let _ = rx.recv().await;

        let (tx, receiver) = broadcast::channel(1);
        m.clone().start_broadcast_listener(receiver).await;
        tx.send(BroadcastEvent::NewBlock(block(11))).unwrap();
        match rx.recv().await.unwrap() {
            WsServerMessage::NewBlock { block } => assert_eq!(block.height, 11),
            other => panic!("expected NewBlock, got {other:?}"),
        }

        // Overrun the one-slot channel so the listener sees a lag, then close it.
        for h in 12..16 {
            let _ = tx.send(BroadcastEvent::NewBlock(block(h)));
        }
        drop(tx);
        // The last event survives the lag; the listener then ends on Closed.
        let mut last = 0;
        while let Some(msg) = rx.recv().await {
            if let WsServerMessage::NewBlock { block } = msg {
                last = block.height;
                if last == 15 {
                    break;
                }
            }
        }
        assert_eq!(last, 15);
    }
}
