use crate::{error::AppError, state::AppState, ApiResult};
use axum::{extract::State, Json};
use randscan_core::{
    approved_token, ApprovedToken, BridgeAsset, BridgeAssetActivity, BridgeState, Supply,
    APPROVED_TOKENS,
};
use randscan_db::{BridgeAssetFlowRow, BridgeBackingBurnRow};

/// GET /api/v1/bridge — the bridge's public state as last read from the node, plus `endpoints`:
/// each trusted emitter as its own chain prints it, with its replay floor.
pub async fn get_bridge(State(state): State<AppState>) -> ApiResult<Json<BridgeState>> {
    // Not refreshed yet (the indexer has not reached the node): report a disabled bridge rather
    // than an error, which is also what a chain without one reports.
    let mut bridge = state.indexer.bridge().await.unwrap_or_default();
    bridge.endpoints = bridge.derive_endpoints();
    Ok(Json(bridge))
}

/// GET /api/v1/bridge/assets — every backing of every bridged token with what the chain has seen
/// of it. The registry comes from the node, the flows from the indexed transactions; a backing
/// with no flow yet lists zeros, and a flow whose token the node has not (yet) registered is
/// skipped. Empty on a chain without a bridge.
pub async fn bridge_assets(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<BridgeAssetActivity>>> {
    let Some(bridge) = state.indexer.bridge().await.filter(|b| b.enabled) else {
        return Ok(Json(vec![]));
    };
    let flows = randscan_db::bridge_asset_flows(state.db.inner()).await?;
    let burns = randscan_db::bridge_backing_burns(state.db.inner()).await?;
    Ok(Json(backing_rows(&bridge.assets, &flows, &burns)))
}

/// One row per backing. A registry index is a *token*, and since chain 14 a token may have
/// several backings, so the per-index flows are laid on a row only where they are that row's:
///
/// - a burn names its coin, `(to_chain, token)`, so burns are matched per backing;
/// - a deposit publishes only the index, so it is a backing's own only when the token has one
///   backing, and `None` otherwise — never the token's total repeated on each row, which a
///   reader summing the rows would count once per backing;
/// - what a backing holds is the registry's `locked`, not a difference of token-wide sums.
///
/// The token's own totals ride along as `token_*`, the same on each of its rows.
fn backing_rows(
    assets: &[BridgeAsset],
    flows: &[BridgeAssetFlowRow],
    burns: &[BridgeBackingBurnRow],
) -> Vec<BridgeAssetActivity> {
    assets
        .iter()
        .map(|a| {
            let backings = assets.iter().filter(|b| b.index == a.index).count() as i64;
            let token = flows.iter().find(|f| f.asset_index == a.index);
            let token_deposited = token.map_or_else(|| "0".to_string(), |f| f.deposited.clone());
            let token_burned = token.map_or_else(|| "0".to_string(), |f| f.burned.clone());
            let token_deposit_fees =
                token.map_or_else(|| "0".to_string(), |f| f.deposit_fees.clone());
            let token_burn_fees = token.map_or_else(|| "0".to_string(), |f| f.burn_fees.clone());
            let own = burns.iter().find(|b| {
                b.asset_index == a.index
                    && b.chain == a.chain
                    && b.token.eq_ignore_ascii_case(&a.token)
            });
            let burned = own.map_or_else(|| "0".to_string(), |b| b.burned.clone());
            let burn_fees = own.map_or_else(|| "0".to_string(), |b| b.burn_fees.clone());
            let sole = backings == 1;
            let known = approved_token(a.chain, &a.token);
            BridgeAssetActivity {
                index: a.index,
                chain: a.chain,
                token: a.token.clone(),
                asset_id: a.asset_id.clone(),
                symbol: known.map(|k| k.symbol.to_string()),
                name: known.map(|k| k.name.to_string()),
                decimals: known.map(|k| k.decimals),
                backings,
                deposits: sole.then(|| token.map_or(0, |f| f.deposits)),
                deposited: sole.then(|| token_deposited.clone()),
                burns: own.map_or(0, |b| b.burns),
                outstanding: a
                    .locked
                    .clone()
                    .unwrap_or_else(|| units_sub(&token_deposited, &burned)),
                burned,
                burn_fees,
                token_deposit_fees,
                token_burn_fees,
                token_deposits: token.map_or(0, |f| f.deposits),
                token_deposited,
                token_burns: token.map_or(0, |f| f.burns),
                token_burned,
                first_height: token.and_then(|f| f.first_height),
                last_height: token.and_then(|f| f.last_height),
                locked: a.locked.clone(),
                minted_today: a.minted_today.clone(),
                mint_cap_per_day: a.mint_cap_per_day.clone(),
                minted_in_window: a.minted_in_window.clone(),
                mint_window_secs: a.mint_window_secs,
                mint_headroom: a.mint_headroom.clone(),
            }
        })
        .collect()
}

/// GET /api/v1/bridge/tokens — the tokens the bridge accepts, with each contract address as the
/// chain prints it and as the registry stores it. A static list; it does not depend on the node.
pub async fn bridge_tokens() -> Json<&'static [ApprovedToken]> {
    Json(APPROVED_TOKENS)
}

/// `a - b` on decimal unit strings, saturating at zero (the node never lets burns exceed
/// deposits, but a partially indexed chain can see a burn before its deposit).
fn units_sub(a: &str, b: &str) -> String {
    let a: u128 = a.parse().unwrap_or(0);
    let b: u128 = b.parse().unwrap_or(0);
    a.saturating_sub(b).to_string()
}

/// GET /api/v1/supply — the node's supply audit (phase S2); 404 on a node without it.
pub async fn get_supply(State(state): State<AppState>) -> ApiResult<Json<Supply>> {
    state
        .indexer
        .supply()
        .await
        .map(Json)
        .ok_or_else(|| AppError::NotFound("supply audit".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backing(index: i64, chain: i64, token: &str, locked: &str) -> BridgeAsset {
        serde_json::from_value(serde_json::json!({
            "index": index, "chain": chain, "token": token, "asset_id": format!("id-{chain}-{token}"),
            "decimals": 6, "locked": locked, "mint_cap_per_day": "10000000000000",
            "minted_today": locked, "mint_day": 20716
        }))
        .unwrap()
    }

    fn token_flow(
        index: i64,
        deposits: i64,
        deposited: &str,
        burns: i64,
        burned: &str,
    ) -> BridgeAssetFlowRow {
        BridgeAssetFlowRow {
            asset_index: index,
            deposits,
            deposited: deposited.into(),
            burns,
            burned: burned.into(),
            deposit_fees: "0".into(),
            burn_fees: "0".into(),
            first_height: Some(500),
            last_height: Some(900),
        }
    }

    /// Chain 14 as it stood on 2026-09-20: zUSD is registry index 1 with seven backings, and three
    /// deposits of 1 zUSD had come in through three different coins. Every row used to report
    /// the whole token's "3 deposits, 3 zUSD outstanding", so the page's total read 21 deposits.
    #[test]
    fn a_tokens_tally_is_not_repeated_on_each_of_its_backings() {
        let one = "100000000";
        let assets = vec![
            backing(1, 2, "e7", "0"),
            backing(1, 2, "e8", "0"),
            backing(1, 3, "b7", one),
            backing(1, 3, "b8", "0"),
            backing(1, 4, "77", one),
            backing(1, 5, "57", one),
            backing(1, 5, "58", "0"),
        ];
        let rows = backing_rows(&assets, &[token_flow(1, 3, "300000000", 0, "0")], &[]);
        assert_eq!(rows.len(), 7);
        for (row, a) in rows.iter().zip(&assets) {
            assert_eq!(row.backings, 7);
            assert_eq!(
                (row.deposits, row.deposited.as_deref()),
                (None, None),
                "not attributable to one coin"
            );
            assert_eq!(
                Some(&row.outstanding),
                a.locked.as_ref(),
                "a backing holds what the registry says"
            );
            assert_eq!(
                (row.token_deposits, row.token_deposited.as_str()),
                (3, "300000000")
            );
        }
        // What the rows hold adds up to what the token's deposits brought in, once.
        let held: u128 = rows
            .iter()
            .map(|r| r.outstanding.parse::<u128>().unwrap())
            .sum();
        assert_eq!(held, 300_000_000);
    }

    #[test]
    fn a_burn_is_laid_on_the_backing_it_names_and_on_no_other() {
        let assets = vec![
            backing(1, 2, "AA", "600"),
            backing(1, 3, "aa", "0"),
            backing(2, 2, "aa", "0"),
        ];
        let burns = vec![BridgeBackingBurnRow {
            asset_index: 1,
            chain: 2,
            token: "aa".into(),
            burns: 1,
            burned: "400".into(),
            burn_fees: "0".into(),
        }];
        let rows = backing_rows(&assets, &[token_flow(1, 1, "1000", 1, "400")], &burns);
        // Matched on the index, the chain and the address (hex case aside): the same address on
        // another chain, or under another token, is another coin.
        assert_eq!((rows[0].burns, rows[0].burned.as_str()), (1, "400"));
        assert_eq!((rows[1].burns, rows[1].burned.as_str()), (0, "0"));
        assert_eq!((rows[2].burns, rows[2].burned.as_str()), (0, "0"));
        // Per-backing burns add up to the token's.
        let per_backing: i64 = rows.iter().filter(|r| r.index == 1).map(|r| r.burns).sum();
        assert_eq!(per_backing, rows[0].token_burns);
    }

    #[test]
    fn a_sole_backing_owns_its_tokens_deposits() {
        let mut old_node = backing(1, 2, "cc", "0");
        old_node.locked = None; // a node before chain 14 serves no `locked`
        let rows = backing_rows(
            &[old_node, backing(2, 2, "dd", "0")],
            &[token_flow(1, 1, "1000", 1, "400")],
            &[BridgeBackingBurnRow {
                asset_index: 1,
                chain: 2,
                token: "cc".into(),
                burns: 1,
                burned: "400".into(),
                burn_fees: "0".into(),
            }],
        );
        assert_eq!(
            (rows[0].deposits, rows[0].deposited.as_deref()),
            (Some(1), Some("1000"))
        );
        assert_eq!(
            rows[0].outstanding, "600",
            "rebuilt from the flows only without the registry's figure"
        );
        assert_eq!(
            (rows[1].deposits, rows[1].deposited.as_deref()),
            (Some(0), Some("0"))
        );
    }

    /// Chain 20 (`bridge.fees` 10/10 bps): one 1 zUSD deposit through Ethereum USDT and a 0.5
    /// zUSD burn back out. The backing holds gross − release; the fees are notes beside it, and
    /// what the rows hold still equals what the chain's zUSD supply is backed by.
    #[test]
    fn under_bridge_fees_a_backing_holds_the_gross_less_the_releases() {
        let assets = vec![backing(1, 2, "e7", "50050000")];
        let mut flow = token_flow(1, 1, "100000000", 1, "49950000");
        flow.deposit_fees = "100000".into();
        flow.burn_fees = "50000".into();
        let burns = vec![BridgeBackingBurnRow {
            asset_index: 1,
            chain: 2,
            token: "e7".into(),
            burns: 1,
            burned: "49950000".into(),
            burn_fees: "50000".into(),
        }];
        let rows = backing_rows(&assets, &[flow], &burns);
        let r = &rows[0];
        assert_eq!(
            r.deposited.as_deref(),
            Some("100000000"),
            "the gross the guardians signed"
        );
        assert_eq!(
            (r.burned.as_str(), r.burn_fees.as_str()),
            ("49950000", "50000")
        );
        assert_eq!(
            (r.token_deposit_fees.as_str(), r.token_burn_fees.as_str()),
            ("100000", "50000")
        );
        assert_eq!(r.outstanding, "50050000");
        // deposited − released == locked, and supply (net notes + fee notes) is the same figure.
        assert_eq!(
            units_sub(r.deposited.as_deref().unwrap(), &r.burned),
            r.outstanding
        );
    }
}
