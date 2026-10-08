//! The RPL token registry (`/tokens`, `/tokens/:id`) — task S1 item 2. Everything here is public
//! registry state read from the node's cached `rand_getTokens` page (`IndexerService::tokens`);
//! nothing here says which notes hold a token, and the supply history is built only from the
//! indexed `register_token`/`token_mint`/`token_burn` transactions, whose amounts are public by
//! design (spec §4). A plain transfer of any token never appears here.

use crate::{error::AppError, state::AppState, ApiResult};
use axum::{
    extract::{Path, State},
    Json,
};
use randscan_core::{TokenDetail, TokenInfo, TokenList, TokenSupplyEvent};
use randscan_db as db;

/// GET /api/v1/tokens — the whole RPL registry as last cached from the node.
pub async fn list_tokens(State(state): State<AppState>) -> ApiResult<Json<TokenList>> {
    Ok(Json(state.indexer.tokens().await.unwrap_or_default()))
}

/// A token named by a registry index (a bare number) or its 64-hex / `rpl1…` id, matched against
/// the cached registry — never a fresh per-token RPC call, which would tell the node which token
/// this reader cares about (`docs/rpc.md`'s privacy note on `rand_getToken`).
fn find_token<'a>(tokens: &'a [TokenInfo], key: &str) -> Option<&'a TokenInfo> {
    let key = key.trim();
    if let Ok(index) = key.parse::<i64>() {
        if let Some(t) = tokens.iter().find(|t| t.index == index) {
            return Some(t);
        }
    }
    let hex_key = key
        .strip_prefix("0x")
        .or_else(|| key.strip_prefix("0X"))
        .unwrap_or(key);
    tokens
        .iter()
        .find(|t| t.id.eq_ignore_ascii_case(hex_key) || t.id_text.eq_ignore_ascii_case(key))
}

/// GET /api/v1/tokens/:id — one token by index, 64-hex id or `rpl1…` text form, its deploy
/// transaction and its public supply history (every `register_token`/`token_mint`/`token_burn`
/// naming it, oldest first).
pub async fn get_token(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<TokenDetail>> {
    let list = state.indexer.tokens().await.unwrap_or_default();
    let info = find_token(&list.tokens, &id)
        .cloned()
        .ok_or_else(|| AppError::NotFound("token".into()))?;

    let pool = state.db.inner();
    let deploy_tx = db::find_token_deploy_tx(pool, info.index, &info.id).await?;
    let events = db::list_token_events(pool, info.index, 1000).await?;
    let supply_history = events
        .into_iter()
        .filter_map(|e| {
            let delta = match e.kind.as_str() {
                // register_token's own row is the whole-token registration, not the initial
                // mint: only fold in an amount when one exists (the `initial_amount` field of
                // its `token_action`; a registration with no initial mint moved no supply).
                "register_token" => e
                    .token_action
                    .as_ref()
                    .and_then(|v| v.get("initial_amount"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                "token_mint" => e.amount.clone(),
                "token_burn" => e.amount.as_deref().map(|a| format!("-{a}")),
                _ => None,
            }?;
            Some(TokenSupplyEvent {
                tx_hash: e.tx_hash,
                height: e.height,
                timestamp_ms: e.timestamp_ms,
                kind: e.kind,
                delta,
            })
        })
        .collect();

    Ok(Json(TokenDetail {
        info,
        deploy_tx,
        supply_history,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(index: i64, id: &str, id_text: &str) -> TokenInfo {
        TokenInfo {
            index,
            id: id.to_string(),
            id_text: id_text.to_string(),
            name: "zUSD".into(),
            symbol: "zUSD".into(),
            decimals: 6,
            authority: randscan_core::TokenAuthority::None,
            mint_nonce: 0,
            total_supply: "0".into(),
            registered_at: 1,
        }
    }

    #[test]
    fn finds_a_token_by_index_hex_id_or_text_form() {
        let id = "aa".repeat(32);
        let tokens = vec![token(3, &id, "rpl1zusd")];
        assert_eq!(find_token(&tokens, "3").unwrap().symbol, "zUSD");
        assert_eq!(
            find_token(&tokens, &id.to_ascii_uppercase()).unwrap().index,
            3
        );
        assert_eq!(find_token(&tokens, &format!("0x{id}")).unwrap().index, 3);
        assert_eq!(find_token(&tokens, "RPL1ZUSD").unwrap().index, 3);
        assert!(find_token(&tokens, "9").is_none());
        assert!(find_token(&tokens, "not-a-token").is_none());
    }
}
