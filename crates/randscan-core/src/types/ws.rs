//! WebSocket protocol.

use serde::{Deserialize, Serialize};

use super::{BlockSummary, NetworkStats, TransactionSummary};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    Subscribe { channel: WsChannel },
    Unsubscribe { channel: WsChannel },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    Subscribed {
        channel: WsChannel,
        subscription_id: String,
    },
    Unsubscribed {
        channel: WsChannel,
    },
    Pong,
    Error {
        message: String,
    },
    NewBlock {
        block: BlockSummary,
    },
    NewTransaction {
        transaction: TransactionSummary,
    },
    StatsUpdate {
        stats: NetworkStats,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsChannel {
    Blocks,
    Transactions,
    Stats,
}

/// Event produced by the indexer and fanned out to subscribers.
#[derive(Debug, Clone)]
pub enum BroadcastEvent {
    NewBlock(BlockSummary),
    NewTransaction(TransactionSummary),
    StatsUpdate(NetworkStats),
}

impl BroadcastEvent {
    pub fn channel(&self) -> WsChannel {
        match self {
            BroadcastEvent::NewBlock(_) => WsChannel::Blocks,
            BroadcastEvent::NewTransaction(_) => WsChannel::Transactions,
            BroadcastEvent::StatsUpdate(_) => WsChannel::Stats,
        }
    }

    pub fn to_message(&self) -> WsServerMessage {
        match self {
            BroadcastEvent::NewBlock(b) => WsServerMessage::NewBlock { block: b.clone() },
            BroadcastEvent::NewTransaction(t) => WsServerMessage::NewTransaction {
                transaction: t.clone(),
            },
            BroadcastEvent::StatsUpdate(s) => WsServerMessage::StatsUpdate { stats: s.clone() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn block() -> BlockSummary {
        serde_json::from_value(json!({
            "hash": "h", "height": 5, "view": 10, "parent": "p", "proposer": "v",
            "timestamp_ms": 1, "tx_count": 0, "justify_view": 9
        }))
        .unwrap()
    }

    fn transaction() -> TransactionSummary {
        serde_json::from_value(json!({
            "hash": "t", "height": 5, "block_hash": "h", "tx_index": 0, "kind": "transfer",
            "fee": "0", "timestamp_ms": 1, "has_bundle": true, "program": null,
            "validator": null, "amount": null, "asset_index": null
        }))
        .unwrap()
    }

    fn stats() -> NetworkStats {
        serde_json::from_value(json!({
            "chain_id": 31, "symbol": "RAND", "decimals": 9, "height": 5, "view": 10,
            "total_transactions": 1, "notes": 2, "nullifiers": 3, "validator_count": 1,
            "active_validator_count": 1, "total_stake": "10", "total_supply": "20",
            "program_count": 0, "avg_block_time_ms": 1000.0, "peer_count": 0,
            "mempool_size": 0, "node_syncing": false, "faucet": true, "confidential": true,
            "updated_at": "now"
        }))
        .unwrap()
    }

    #[test]
    fn every_event_belongs_to_its_own_channel() {
        assert_eq!(
            BroadcastEvent::NewBlock(block()).channel(),
            WsChannel::Blocks
        );
        assert_eq!(
            BroadcastEvent::NewTransaction(transaction()).channel(),
            WsChannel::Transactions
        );
        assert_eq!(
            BroadcastEvent::StatsUpdate(stats()).channel(),
            WsChannel::Stats
        );
    }

    #[test]
    fn events_become_the_message_the_client_expects() {
        let m = serde_json::to_value(BroadcastEvent::NewBlock(block()).to_message()).unwrap();
        assert_eq!(m["type"], "new_block");
        assert_eq!(m["block"]["height"], 5);
        let m = serde_json::to_value(BroadcastEvent::NewTransaction(transaction()).to_message())
            .unwrap();
        assert_eq!(m["type"], "new_transaction");
        assert_eq!(m["transaction"]["kind"], "transfer");
        let m = serde_json::to_value(BroadcastEvent::StatsUpdate(stats()).to_message()).unwrap();
        assert_eq!(m["type"], "stats_update");
        assert_eq!(m["stats"]["chain_id"], 31);
    }

    #[test]
    fn client_messages_parse_from_their_wire_form() {
        let sub: WsClientMessage =
            serde_json::from_str(r#"{"type":"subscribe","channel":"blocks"}"#).unwrap();
        assert!(matches!(
            sub,
            WsClientMessage::Subscribe {
                channel: WsChannel::Blocks
            }
        ));
        let unsub: WsClientMessage =
            serde_json::from_str(r#"{"type":"unsubscribe","channel":"stats"}"#).unwrap();
        assert!(matches!(
            unsub,
            WsClientMessage::Unsubscribe {
                channel: WsChannel::Stats
            }
        ));
        assert!(matches!(
            serde_json::from_str::<WsClientMessage>(r#"{"type":"ping"}"#).unwrap(),
            WsClientMessage::Ping
        ));
        assert!(serde_json::from_str::<WsClientMessage>(
            r#"{"type":"subscribe","channel":"nope"}"#
        )
        .is_err());
    }
}
