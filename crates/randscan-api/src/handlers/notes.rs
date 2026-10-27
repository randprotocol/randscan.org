use crate::{error::AppError, state::AppState, ApiResult};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use randscan_core::{
    classify_query, Note, Nullifier, PaginatedResponse, Pagination, PaginationInfo, QueryKind,
};
use randscan_db as db;

/// GET /api/v1/notes — the commitment tree, newest leaf first.
pub async fn list_notes(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> ApiResult<Json<PaginatedResponse<Note>>> {
    let pool = state.db.inner();
    let rows = db::list_notes(pool, p.offset(), p.limit()).await?;
    let total = db::count_notes(pool).await?;
    Ok(Json(PaginatedResponse {
        data: rows.into_iter().map(Into::into).collect(),
        pagination: PaginationInfo::new(&p, total),
    }))
}

/// GET /api/v1/notes/:id — a leaf by commitment (64 hex) or by leaf index (decimal).
pub async fn get_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Note>> {
    let pool = state.db.inner();
    let row = match classify_query(&id) {
        QueryKind::Hash(cm) => db::get_note_by_cm(pool, &cm).await?,
        QueryKind::Height(index) => db::get_note_by_index(pool, index).await?,
        _ => {
            return Err(AppError::BadRequest(
                "note id must be a commitment (64 hex) or a leaf index".into(),
            ))
        }
    };
    Ok(Json(
        row.ok_or_else(|| AppError::NotFound("note".into()))?.into(),
    ))
}

/// GET /api/v1/nullifiers/:nf — the transaction that published this nullifier.
pub async fn get_nullifier(
    State(state): State<AppState>,
    Path(nf): Path<String>,
) -> ApiResult<Json<Nullifier>> {
    let nf = match classify_query(&nf) {
        QueryKind::Hash(h) => h,
        _ => {
            return Err(AppError::BadRequest(
                "nullifier must be 64 hex characters".into(),
            ))
        }
    };
    let row = db::get_nullifier(state.db.inner(), &nf)
        .await?
        .ok_or_else(|| AppError::NotFound("nullifier".into()))?;
    Ok(Json(row.into()))
}

/// The most nullifiers one `GET /api/v1/nullifiers` page may carry.
pub const MAX_NULLIFIER_PAGE: i64 = 5000;

#[derive(Debug, serde::Deserialize)]
pub struct NullifierPageQuery {
    /// Return nullifiers strictly after this one (64 hex); omitted = from the start.
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct NullifierPage {
    pub nullifiers: Vec<Nullifier>,
    /// Pass as `after` for the next page; `None` on the last.
    pub next: Option<String>,
}

/// GET /api/v1/nullifiers?after&limit — the published nullifier set, a page at a time. There is
/// deliberately no way to ask about particular nullifiers in bulk: a nullifier is derived from a
/// wallet's key, so naming one tells the server which notes a visitor owns. A client downloads the
/// pages and matches its own locally.
pub async fn list_nullifiers(
    State(state): State<AppState>,
    Query(q): Query<NullifierPageQuery>,
) -> ApiResult<Json<NullifierPage>> {
    let limit = q
        .limit
        .unwrap_or(MAX_NULLIFIER_PAGE)
        .clamp(1, MAX_NULLIFIER_PAGE);
    let after = match q.after.as_deref() {
        None => None,
        Some(a) => match classify_query(a) {
            QueryKind::Hash(h) => Some(h),
            _ => {
                return Err(AppError::BadRequest(
                    "after must be 64 hex characters".into(),
                ))
            }
        },
    };
    let rows = db::list_nullifiers_after(state.db.inner(), after.as_deref(), limit).await?;
    let next = if rows.len() as i64 == limit {
        rows.last().map(|r| r.nullifier.clone())
    } else {
        None
    };
    Ok(Json(NullifierPage {
        nullifiers: rows.into_iter().map(Into::into).collect(),
        next,
    }))
}
