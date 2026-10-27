// "Is this page on the live chain and up to date": the site-sync classifier (design §3), the same
// rules as the API's `randscan_core::classify` and every other Rand site's copy. Pure, no React,
// so tests/chain-status.test.mjs runs it under plain Node.

import type { ChainPin } from './chainPin';

/** A head older than this is stalled: 24 blocks at the chain's 2.5 s block time. */
export const STALL_MS = 60_000;
/** An indexer more than this many blocks behind its node is behind. */
export const MAX_LAG = 10;
/** How often the header polls /api/v1/chain. */
export const POLL_MS = 15_000;
/** A /chain request that takes longer than this counts as unreachable. */
export const FETCH_TIMEOUT_MS = 8_000;

export type ChainState = 'unreachable' | 'wrong_chain' | 'stalled' | 'behind' | 'live';

/** What GET /api/v1/chain answers (the ChainView plus the server's own verdict). */
export interface ChainView {
  chain_id: number;
  genesis_hash: string | null;
  head_height: number;
  head_timestamp_ms: number | null;
  node_height?: number | null;
  lag?: number | null;
  node_chain_id?: number | null;
  node_genesis_hash?: string | null;
  /** The explorer's indexer stopped writing because its node moved to another chain. */
  halted?: boolean;
  /** Whether the indexer's last request to its node succeeded. */
  node_connected?: boolean;
  state?: ChainState;
  detail?: string;
}

/** Why the state is what it is; each one is a banner sentence in the dictionary. */
export type ChainReason =
  | 'fetch' // the page could not read /chain at all
  | 'node' // the API answered, but the explorer cannot reach its node
  | 'chainId' // a different chain id
  | 'genesis' // the same id (or none) with another or no genesis
  | 'halted' // the node moved to another chain; indexing stopped
  | 'noBlock' // nothing indexed yet
  | 'stale' // no new block for more than STALL_MS
  | 'lag' // more than MAX_LAG blocks behind the node
  | 'ok';

export interface Classification {
  state: ChainState;
  reason: ChainReason;
  /** One English sentence, for logs and tooltips; the page shows the translated reason. */
  detail: string;
}

export const normalizeHash = (h: string): string => h.trim().replace(/^0x/i, '').toLowerCase();
const sameHash = (a: string, b: string) => normalizeHash(a) === normalizeHash(b);

/**
 * Checked in order: unreachable, wrong chain, stalled, behind, live. `view` is `null` (or an
 * Error) when the fetch failed, timed out or did not parse.
 */
export function classify(
  view: ChainView | null | Error | undefined,
  expected: ChainPin,
  nowMs: number,
): Classification {
  if (!view || view instanceof Error || typeof view.chain_id !== 'number') {
    return { state: 'unreachable', reason: 'fetch', detail: 'The chain source could not be reached.' };
  }
  const genesisOk = !!view.genesis_hash && sameHash(view.genesis_hash, expected.genesis_hash);
  const nodeMoved =
    (view.node_chain_id != null && view.node_chain_id !== expected.chain_id) ||
    (view.node_genesis_hash != null && !sameHash(view.node_genesis_hash, expected.genesis_hash));
  if (view.chain_id !== expected.chain_id || !genesisOk || nodeMoved) {
    const reason: ChainReason =
      view.chain_id !== expected.chain_id ? 'chainId' : !genesisOk ? 'genesis' : 'halted';
    return {
      state: 'wrong_chain',
      reason,
      detail: `Showing chain ${view.chain_id}; expected chain ${expected.chain_id}.`,
    };
  }
  if (view.head_timestamp_ms == null) {
    return { state: 'stalled', reason: 'noBlock', detail: 'No block has been indexed yet.' };
  }
  if (nowMs - view.head_timestamp_ms > STALL_MS) {
    const s = Math.round((nowMs - view.head_timestamp_ms) / 1000);
    return { state: 'stalled', reason: 'stale', detail: `No new block for ${s} s since #${view.head_height}.` };
  }
  // The data was current a moment ago, but the explorer cannot confirm it with its node.
  if (view.node_connected === false) {
    return { state: 'unreachable', reason: 'node', detail: "The explorer can't reach its node." };
  }
  if (view.lag != null && view.lag > MAX_LAG) {
    return { state: 'behind', reason: 'lag', detail: `${view.lag} blocks behind the node.` };
  }
  return { state: 'live', reason: 'ok', detail: `Live at #${view.head_height}.` };
}

/** Pill colour: green when live, amber when behind, red otherwise. */
export function tone(state: ChainState | null): 'positive' | 'warning' | 'negative' | 'mute' {
  if (state === null) return 'mute';
  if (state === 'live') return 'positive';
  if (state === 'behind') return 'warning';
  return 'negative';
}

/** The banner shows for every state but live (behind means more than MAX_LAG blocks). */
export const showsBanner = (state: ChainState | null): boolean => state !== null && state !== 'live';

/** What the banner says for a classification: a dictionary key, its variables, and the last good
 *  block (height and timestamp) when there is one worth naming. `age` is filled by the caller,
 *  which owns the locale's relative-time formatter. */
export interface BannerMessage {
  key: string;
  vars: Record<string, string | number>;
  lastGood: { height: number; timestampMs: number } | null;
}

export function bannerMessage(
  status: Classification,
  view: ChainView | null | undefined,
  expected: ChainPin,
): BannerMessage | null {
  if (!showsBanner(status.state)) return null;
  return {
    key: `chainStatus.banner.${status.reason}`,
    vars: {
      expected: expected.chain_id,
      shown: view?.chain_id ?? '?',
      genesis: view?.genesis_hash ? `${normalizeHash(view.genesis_hash).slice(0, 8)}…` : '—',
      lag: view?.lag ?? 0,
    },
    lastGood:
      view && view.head_timestamp_ms != null && status.reason !== 'noBlock'
        ? { height: view.head_height, timestampMs: view.head_timestamp_ms }
        : null,
  };
}
