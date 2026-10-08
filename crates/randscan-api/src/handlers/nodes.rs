use crate::{state::AppState, ApiResult};
use axum::{extract::State, Json};
use randscan_core::NodeInfo;

/// GET /api/v1/nodes
pub async fn list_nodes(State(state): State<AppState>) -> ApiResult<Json<Vec<NodeInfo>>> {
    Ok(Json(state.indexer.nodes().await))
}

/// GET /api/v1/provers — the delegated provers the explorer knows of (`KNOWN_PROVERS`), each with
/// its last `prover_info` (whether it answers, its queue, its fee) and its member hosts placed on
/// the map by geolocation; the hosts' addresses are not served.
pub async fn list_provers(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<randscan_core::ProverView>>> {
    Ok(Json(state.indexer.provers().await))
}
