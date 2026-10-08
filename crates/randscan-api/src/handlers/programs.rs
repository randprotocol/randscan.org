use crate::{error::AppError, state::AppState, ApiResult};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use randscan_core::{
    classify_query, PaginatedResponse, Pagination, PaginationInfo, ProgramCellsPage, ProgramDetail,
    ProgramState, ProgramSummary, QueryKind,
};
use randscan_db as db;
use serde::Deserialize;
use tracing::warn;

/// How many cells the program page carries; `GET /programs/:id/cells` pages the rest.
const CELLS_FIRST_PAGE: u64 = 100;

/// GET /api/v1/programs
pub async fn list_programs(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> ApiResult<Json<PaginatedResponse<ProgramSummary>>> {
    let pool = state.db.inner();
    let rows = db::list_programs(pool, p.offset(), p.limit()).await?;
    let total = db::count_programs(pool).await?;
    Ok(Json(PaginatedResponse {
        data: rows.into_iter().map(Into::into).collect(),
        pagination: PaginationInfo::new(&p, total),
    }))
}

/// GET /api/v1/programs/:id
pub async fn get_program(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ProgramDetail>> {
    let id = match classify_query(&id) {
        QueryKind::Hash(h) => h,
        _ => {
            return Err(AppError::BadRequest(
                "program id must be 64 hex characters".into(),
            ))
        }
    };
    let pool = state.db.inner();
    let row = db::get_program(pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound("program".into()))?;
    let calls = db::list_program_calls(pool, &id, 10).await?;
    // RPL-2: the vault and the first page of cells, live from the node — program state is
    // public chain state that moves with every invoke, and the node serves it paged in key
    // order. `None` on a chain without the section (and on an older node); a failed read is
    // logged and served as `None` rather than failing the page.
    let rpc = state.indexer.rpc();
    let program_state = match (
        rpc.program_vault(&id).await,
        rpc.program_cells(&id, None, CELLS_FIRST_PAGE).await,
    ) {
        (Ok(Some(vault)), Ok(Some(page))) => Some(ProgramState {
            vault,
            cells: page.cells,
            cells_next: page.next,
        }),
        (Ok(_), Ok(_)) => None,
        (Err(e), _) | (_, Err(e)) => {
            warn!("program state read for {id} failed: {e:#}");
            None
        }
    };
    Ok(Json(ProgramDetail {
        program: row.into(),
        recent_calls: calls.into_iter().map(Into::into).collect(),
        program_state,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CellsQuery {
    /// the `cells_next` / `next` of the previous page
    pub after: Option<String>,
    pub limit: Option<u64>,
}

/// GET /api/v1/programs/:id/cells — a page of a program's cells in key order (RPL-2,
/// `rand_getProgramCells`), straight from the node. 404 on a chain without a `program_state`
/// section.
pub async fn program_cells(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<CellsQuery>,
) -> ApiResult<Json<ProgramCellsPage>> {
    let id = match classify_query(&id) {
        QueryKind::Hash(h) => h,
        _ => {
            return Err(AppError::BadRequest(
                "program id must be 64 hex characters".into(),
            ))
        }
    };
    let after = match q.after.as_deref().filter(|a| !a.is_empty()) {
        Some(a) => match classify_query(a) {
            QueryKind::Hash(h) => Some(h),
            _ => {
                return Err(AppError::BadRequest(
                    "after must be a 64 hex cell key".into(),
                ))
            }
        },
        None => None,
    };
    let limit = q.limit.unwrap_or(CELLS_FIRST_PAGE).clamp(1, 1000);
    let page = state
        .indexer
        .rpc()
        .program_cells(&id, after.as_deref(), limit)
        .await
        .map_err(|e| AppError::Internal(format!("program cells: {e:#}")))?
        .ok_or_else(|| {
            AppError::NotFound("program state (this chain has no program_state section)".into())
        })?;
    Ok(Json(page))
}
