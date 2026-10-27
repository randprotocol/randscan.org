use super::chain::{chain_view, now_ms};
use crate::{state::AppState, ApiResult};
use axum::{extract::State, Json};
use randscan_core::{is_stalled, is_wrong_chain, HealthResponse, IndexerHealth};
use randscan_db as db;

/// GET /api/v1/health
///
/// `status` is `healthy` when the database answers and the indexer is connected, synced, fresh
/// and on the pinned chain; `degraded` when the database answers but any of that fails (a stalled
/// head or the wrong chain included); `unhealthy` without the database.
pub async fn health(State(state): State<AppState>) -> ApiResult<Json<HealthResponse>> {
    let db_ok = db::get_network_stats(state.db.inner()).await.is_ok();
    let s = state.indexer.get_state().await;
    let lag = (s.node_height - s.current_height).max(0);
    let indexer = IndexerHealth {
        connected: s.is_connected,
        synced: s.is_connected && !s.is_syncing && lag <= 2,
        current_height: s.current_height,
        node_height: s.node_height,
        lag,
    };
    let view = chain_view(&state).await.ok().map(|(v, _, _)| v);
    let stalled = view.as_ref().is_none_or(|v| is_stalled(v, now_ms()));
    let wrong_chain = view
        .as_ref()
        .is_some_and(|v| is_wrong_chain(v, state.indexer.expected()));
    let status = if db_ok && indexer.connected && indexer.synced && !stalled && !wrong_chain {
        "healthy"
    } else if db_ok {
        "degraded"
    } else {
        "unhealthy"
    };
    Ok(Json(HealthResponse {
        status: status.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        database: db_ok,
        indexer,
        genesis_hash: view.as_ref().and_then(|v| v.genesis_hash.clone()),
        head_timestamp_ms: view.as_ref().and_then(|v| v.head_timestamp_ms),
        stalled,
        wrong_chain,
    }))
}
