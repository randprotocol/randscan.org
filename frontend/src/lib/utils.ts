import type { TokenInfo, TransactionKind } from '@/types';

import { makeFormat, type Format } from '@/i18n/format';

export const TOKEN_SYMBOL = 'RAND';
export const TOKEN_DECIMALS = 9;

/** The English formatters; a page's own come from useFmt(). */
const EN: Format = makeFormat('en');

// ---------------------------------------------------------------------------
// Class names
// ---------------------------------------------------------------------------

export function cn(...classes: (string | undefined | null | false)[]): string {
  return classes.filter(Boolean).join(' ');
}

// ---------------------------------------------------------------------------
// Time — all chain timestamps are milliseconds since the epoch
// ---------------------------------------------------------------------------

/** Relative age of a millisecond timestamp, e.g. "12s ago". */
export function formatTimestamp(timestampMs: number): string {
  return EN.ago(timestampMs);
}

/** Absolute rendering of a millisecond timestamp. */
export function formatDateTime(timestampMs: number): string {
  return EN.dateTime(timestampMs);
}

/** Human duration for a millisecond span, e.g. "1.20s" or "2m 5s". */
export function formatDurationMs(ms: number): string {
  return EN.duration(ms);
}

// ---------------------------------------------------------------------------
// Amounts — decimal strings of smallest units
// ---------------------------------------------------------------------------

function toBigInt(value: string | number | bigint): bigint | null {
  try {
    if (typeof value === 'bigint') return value;
    if (typeof value === 'number') return BigInt(Math.trunc(value));
    const trimmed = value.trim();
    if (trimmed === '') return null;
    return BigInt(trimmed);
  } catch {
    return null;
  }
}

/**
 * Convert smallest units to a decimal string with up to `decimals` places,
 * trailing zeros trimmed. Integer part is grouped with commas.
 */
export function formatUnits(
  value: string | number | bigint | null | undefined,
  decimals: number = TOKEN_DECIMALS
): string {
  return EN.units(value, decimals);
}

/** Same as formatUnits, suffixed with the token symbol. */
export function formatAmount(
  value: string | number | bigint | null | undefined,
  decimals: number = TOKEN_DECIMALS,
  symbol: string = TOKEN_SYMBOL
): string {
  return EN.amount(value, decimals, symbol);
}

/**
 * Stake is RAND that left the pool into the validator register, so it is units with the
 * token's nine decimals. `suffix` is appended after the symbol ("1,000 RAND total stake").
 */
export function formatStake(
  value: string | number | bigint | null | undefined,
  suffix = ''
): string {
  return EN.stake(value, suffix);
}

// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

export function formatNumber(n: number | bigint | null | undefined): string {
  return EN.number(n);
}

export function formatCompactNumber(n: number): string {
  return EN.compact(n);
}

export function formatPercentage(value: number, decimals: number = 2): string {
  return EN.percentage(value, decimals);
}

/** Byte counts, used for zk proof sizes. */
export function formatBytes(bytes: number | null | undefined): string {
  return EN.bytes(bytes);
}

/**
 * Deposits and burns across bridge rows, each token counted once. The rows are per *backing* and
 * a token with several backings repeats its `token_*` tallies on each of them, so summing rows
 * would count a deposit once per backing.
 */
export function bridgeTokenTotals(
  rows: { index: number; token_deposits: number; token_burns: number }[],
): { deposits: number; burns: number } {
  const seen = new Set<number>();
  let deposits = 0;
  let burns = 0;
  for (const r of rows) {
    if (seen.has(r.index)) continue;
    seen.add(r.index);
    deposits += r.token_deposits;
    burns += r.token_burns;
  }
  return { deposits, burns };
}

/**
 * A genesis size cap in binary units, whole where it is whole: 8388608 is "8 MiB", 65536 is
 * "64 KiB". The caps are powers of two, so `formatBytes`' two decimals would only add noise.
 */
export function formatBinaryBytes(bytes: number | null | undefined): string {
  return EN.binaryBytes(bytes);
}

/** Uptime in seconds rendered as e.g. "3h 12m", "5m 3s", "2d 4h". */
export function formatConnectedTime(seconds: number | null | undefined): string {
  return EN.connected(seconds);
}

// ---------------------------------------------------------------------------
// Hashes and addresses
// ---------------------------------------------------------------------------

export function shortenHash(
  hash: string | null | undefined,
  startChars: number = 8,
  endChars: number = 6
): string {
  if (!hash) return '—';
  if (hash.length <= startChars + endChars + 3) return hash;
  return `${hash.slice(0, startChars)}…${hash.slice(-endChars)}`;
}

const HEX64 = /^(0x)?[0-9a-fA-F]{64}$/;
const BASE58 = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;
const DECIMAL = /^\d+$/;

export function isHash(value: string): boolean {
  return HEX64.test(value.trim());
}

export function isAddress(value: string): boolean {
  const v = value.trim();
  // A 64-hex string is a hash, not an address, even though it is base58-shaped.
  return !HEX64.test(v) && BASE58.test(v);
}

export function isHeight(value: string): boolean {
  return DECIMAL.test(value.trim());
}

export type SearchQueryType = 'height' | 'hash' | 'address' | 'unknown';

export function getSearchQueryType(query: string): SearchQueryType {
  const trimmed = query.trim();
  if (trimmed === '') return 'unknown';
  if (isHeight(trimmed)) return 'height';
  if (isHash(trimmed)) return 'hash';
  if (isAddress(trimmed)) return 'address';
  return 'unknown';
}

// ---------------------------------------------------------------------------
// Transaction kinds
// ---------------------------------------------------------------------------

/** Kinds the API accepts as a `?kind=` filter (`other` is display-only). */
export const TRANSACTION_KINDS: TransactionKind[] = [
  'transfer',
  'mint',
  'deploy',
  'call',
  'bond',
  'unbond',
  'withdraw',
  'bridge_attest',
  'bridge_burn',
  'register_token',
  'token_mint',
  'set_authority',
  'token_burn',
  'pause_mints',
  'unpause_mints',
  'register_bridged_token',
  'list_backing',
  'invoke',
  'admit_validator',
  'slash_equivocation',
  'rotate_pq_guardians',
  'rotate_pause_key',
  'rotate_pq_guardians_v2',
  'rotate_pause_key_v2',
  'cancel_rotation',
  'perp_deposit',
  'perp_order',
  'perp_cancel',
  'perp_withdraw',
  'perp_oracle',
  'perp_state_proof',
  'create_multisig',
  'multisig_deposit',
  'multisig_pay',
  'multisig_rotate',
  'set_rand_price',
  'aggregate',
];

const KIND_LABELS: Record<TransactionKind, string> = {
  transfer: 'Transfer (shielded)',
  mint: 'Mint (faucet)',
  deploy: 'Deploy',
  call: 'Call (confidential)',
  bond: 'Bond',
  unbond: 'Unbond',
  withdraw: 'Withdraw',
  bridge_attest: 'Bridge in (attestation)',
  bridge_burn: 'Bridge out (burn)',
  register_token: 'Token registered',
  token_mint: 'Token mint',
  set_authority: 'Token authority changed',
  token_burn: 'Token burn',
  pause_mints: 'Bridge mints paused',
  unpause_mints: 'Bridge mints unpaused',
  register_bridged_token: 'Bridged token listed',
  list_backing: 'Backing listed',
  invoke: 'Invoke (program state)',
  admit_validator: 'Validator admitted (vote)',
  slash_equivocation: 'Equivocation slashed',
  rotate_pq_guardians: 'PQ guardians rotated',
  rotate_pause_key: 'Pause key rotated',
  rotate_pq_guardians_v2: 'PQ guardian rotation (delayed)',
  rotate_pause_key_v2: 'Pause key rotation (delayed)',
  cancel_rotation: 'Rotation cancelled',
  perp_deposit: 'Perp deposit',
  perp_order: 'Perp order',
  perp_cancel: 'Perp order cancelled',
  perp_withdraw: 'Perp withdrawal request',
  perp_oracle: 'Perp oracle prices',
  perp_state_proof: 'Perp state proof',
  create_multisig: 'Multisig account created',
  multisig_deposit: 'Multisig deposit',
  multisig_pay: 'Multisig payment',
  multisig_rotate: 'Multisig signers rotated',
  set_rand_price: 'RAND price vote',
  aggregate: 'Aggregate proof',
  other: 'Other',
};

const KIND_SHORT_LABELS: Record<TransactionKind, string> = {
  transfer: 'Transfer',
  mint: 'Mint',
  deploy: 'Deploy',
  call: 'Call',
  bond: 'Bond',
  unbond: 'Unbond',
  withdraw: 'Withdraw',
  bridge_attest: 'Bridge in',
  bridge_burn: 'Bridge out',
  register_token: 'Register token',
  token_mint: 'Token mint',
  set_authority: 'Set authority',
  token_burn: 'Token burn',
  pause_mints: 'Pause mints',
  unpause_mints: 'Unpause mints',
  register_bridged_token: 'List token',
  list_backing: 'List backing',
  invoke: 'Invoke',
  admit_validator: 'Admit',
  slash_equivocation: 'Slash',
  rotate_pq_guardians: 'Rotate PQ',
  rotate_pause_key: 'Rotate pause',
  rotate_pq_guardians_v2: 'Rotate PQ',
  rotate_pause_key_v2: 'Rotate pause',
  cancel_rotation: 'Cancel rotation',
  perp_deposit: 'Perp deposit',
  perp_order: 'Order',
  perp_cancel: 'Cancel order',
  perp_withdraw: 'Perp withdraw',
  perp_oracle: 'Oracle',
  perp_state_proof: 'State proof',
  create_multisig: 'Create multisig',
  multisig_deposit: 'Multisig deposit',
  multisig_pay: 'Multisig pay',
  multisig_rotate: 'Rotate signers',
  set_rand_price: 'Price vote',
  aggregate: 'Aggregate',
  other: 'Other',
};

const KIND_BADGE_CLASSES: Record<TransactionKind, string> = {
  transfer: 'badge badge-transfer',
  mint: 'badge badge-mint',
  deploy: 'badge badge-deploy',
  call: 'badge badge-call',
  bond: 'badge badge-stake',
  unbond: 'badge badge-stake',
  withdraw: 'badge badge-stake',
  bridge_attest: 'badge badge-bridge',
  bridge_burn: 'badge badge-bridge',
  register_token: 'badge badge-mint',
  token_mint: 'badge badge-mint',
  set_authority: 'badge badge-neutral',
  token_burn: 'badge badge-bridge',
  pause_mints: 'badge badge-neutral',
  unpause_mints: 'badge badge-neutral',
  register_bridged_token: 'badge badge-bridge',
  list_backing: 'badge badge-bridge',
  invoke: 'badge badge-call',
  admit_validator: 'badge badge-stake',
  slash_equivocation: 'badge badge-stake',
  rotate_pq_guardians: 'badge badge-bridge',
  rotate_pause_key: 'badge badge-bridge',
  rotate_pq_guardians_v2: 'badge badge-bridge',
  rotate_pause_key_v2: 'badge badge-bridge',
  cancel_rotation: 'badge badge-bridge',
  perp_deposit: 'badge badge-bridge',
  perp_order: 'badge badge-call',
  perp_cancel: 'badge badge-neutral',
  perp_withdraw: 'badge badge-bridge',
  perp_oracle: 'badge badge-stake',
  perp_state_proof: 'badge badge-deploy',
  create_multisig: 'badge badge-mint',
  multisig_deposit: 'badge badge-transfer',
  multisig_pay: 'badge badge-transfer',
  multisig_rotate: 'badge badge-neutral',
  set_rand_price: 'badge badge-stake',
  aggregate: 'badge badge-deploy',
  other: 'badge badge-neutral',
};

/** Kinds whose `amount` is in a bridged asset's own unit rather than RAND units. */
export function amountIsBridged(kind: string): boolean {
  return kind === 'bridge_attest' || kind === 'bridge_burn';
}

// ---------------------------------------------------------------------------
// RPL tokens
// ---------------------------------------------------------------------------

/** The registry row for `index`, or `undefined` when this build's cached registry does not (yet)
 * know it — a token listed after this page loaded, or a node this explorer has never resolved. */
export function resolveToken(
  tokens: TokenInfo[] | null | undefined,
  index: number | null | undefined
): TokenInfo | undefined {
  if (index === null || index === undefined || !tokens) return undefined;
  return tokens.find((t) => t.index === index);
}

/**
 * An RPL token amount rendered with its registered symbol and decimals: "12.50 zUSD". Falls back
 * to "N units of asset #N" when the registry does not (yet) know the index — never assumes RAND's
 * decimals, since an RPL token's are whatever it registered with.
 */
export function formatTokenAmount(
  units: string | number | bigint | null | undefined,
  index: number | null | undefined,
  tokens: TokenInfo[] | null | undefined,
  fmt: Format = EN
): string {
  if (units === null || units === undefined) return '—';
  if (index === null || index === undefined || index === 0) return fmt.amount(units);
  const token = resolveToken(tokens, index);
  if (!token) return fmt.assetUnits(units, index);
  return `${fmt.units(units, token.decimals)} ${token.symbol}`;
}

/**
 * The tokens a balance list always shows, held or not — as a wallet lists RAND at zero. Today
 * that is zUSD, found by its bridge authority rather than by index (an index is a chain's
 * registration order and moves at a chain cut) or by symbol alone (anyone may register a token
 * called "zUSD"; only governance lists a bridge-backed one). Empty when the registry has none.
 */
export function defaultTokens(tokens: TokenInfo[] | null | undefined): TokenInfo[] {
  const zusd = (tokens ?? [])
    .filter((t) => t.symbol === 'zUSD' && t.authority.kind === 'bridge')
    .sort((a, b) => a.index - b.index)[0];
  return zusd ? [zusd] : [];
}

/** One line of a balance list: what a key's unspent notes of one asset add up to. */
export interface TokenBalance {
  /** The `asset` word — 0 is RAND. */
  asset: number;
  token?: TokenInfo;
  units: bigint;
  notes: number;
}

/** The balance list of `unspent` notes: RAND first, then the default tokens (zUSD) whether any
 * is held or not, then every other asset a note carries, by index. */
export function tokenBalances(
  unspent: { asset: number; amount: string }[],
  tokens: TokenInfo[] | null | undefined
): TokenBalance[] {
  const held = new Map<number, { units: bigint; notes: number }>();
  for (const n of unspent) {
    const h = held.get(n.asset) ?? { units: BigInt(0), notes: 0 };
    held.set(n.asset, { units: h.units + BigInt(n.amount), notes: h.notes + 1 });
  }
  const listed = [0, ...defaultTokens(tokens).map((t) => t.index)];
  const others = Array.from(held.keys()).filter((a) => !listed.includes(a)).sort((a, b) => a - b);
  return [...listed, ...others].map((asset) => ({
    asset,
    token: resolveToken(tokens, asset),
    units: held.get(asset)?.units ?? BigInt(0),
    notes: held.get(asset)?.notes ?? 0,
  }));
}

// ---------------------------------------------------------------------------
// Bridge
// ---------------------------------------------------------------------------

/**
 * Bridged assets used to carry 8 decimals on the account chain; on the shielded chain a
 * bridged amount is in the asset's own smallest unit (see formatTokenAmount). Kept for the
 * legacy rendering of a two-part "tokens (units)" string.
 */
export const BRIDGED_DECIMALS = 8;

const BRIDGE_CHAIN_NAMES: Record<number, string> = {
  1: 'Rand',
  2: 'Ethereum',
  3: 'BSC',
  4: 'Tron',
  5: 'Solana',
};

/** The source chains the bridge watches, in the order the bridge page lists them. */
export const BRIDGE_SOURCE_CHAINS: { id: number; name: string }[] = [
  { id: 2, name: 'Ethereum' },
  { id: 3, name: 'BSC' },
  { id: 5, name: 'Solana' },
  { id: 4, name: 'Tron' },
];

/** "Ethereum" for a known bridge chain id, "chain 9" otherwise. */
/** The name of a known bridge chain, or null; pages pass it to fmt.bridgeChain, which words the fallback. */
export function knownBridgeChainName(id: number | null | undefined): string | null {
  if (id === null || id === undefined) return null;
  return BRIDGE_CHAIN_NAMES[id] ?? null;
}

export function bridgeChainName(id: number | null | undefined): string {
  if (id === null || id === undefined) return '—';
  return BRIDGE_CHAIN_NAMES[id] ?? `chain ${id}`;
}

/**
 * An amount in bridge units (8 decimals, whatever the token's own decimals at home), with the
 * token symbol when the asset is a known one: "1,250.5 USDT", "1,250.5 units".
 */
export function formatBridgeUnits(
  units: string | null | undefined,
  symbol?: string | null
): string {
  return EN.bridgeUnits(units, symbol);
}

/** A mint-cap window in the largest unit that divides it: "24 hours", "90 minutes", "45 seconds". */
export function formatMintWindow(secs: number): string {
  return EN.mintWindow(secs);
}

/** "Ethereum (2)" for a known bridge chain id, "chain 9" otherwise. */
export function formatBridgeChain(id: number | null | undefined): string {
  return EN.bridgeChain(id, knownBridgeChainName(id));
}

/** Bridged units as a decimal number of tokens (8 decimals) with the raw units alongside. */
export function formatBridgedAmount(units: string | null | undefined): string {
  return EN.bridged(units);
}

/**
 * Bridge destinations are 32-byte hex; a 20-byte EVM/Tron address arrives left-padded with
 * 24 zero hex digits. Strip that padding for display and keep the full value for copying.
 */
export function formatBridgeAddress(hex: string): string {
  const clean = hex.toLowerCase().replace(/^0x/, '');
  if (clean.length === 64 && clean.startsWith('0'.repeat(24))) {
    return `0x${clean.slice(24)}`;
  }
  return clean;
}

export function getKindLabel(kind: string): string {
  return KIND_LABELS[kind as TransactionKind] ?? kind;
}

export function getKindShortLabel(kind: string): string {
  return KIND_SHORT_LABELS[kind as TransactionKind] ?? kind;
}

export function getKindBadgeClass(kind: string): string {
  return KIND_BADGE_CLASSES[kind as TransactionKind] ?? 'badge badge-neutral';
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

interface GeoLike {
  city: string | null;
  region: string | null;
  country: string | null;
}

/** "Singapore, SG" style location line; null geo becomes "Unknown location". */
export function formatLocation(geo: GeoLike | null | undefined): string {
  return EN.location(geo);
}

/** "1.2.3.4:9000", falling back gracefully when either half is missing. */
export function formatEndpoint(
  ip: string | null | undefined,
  port: number | null | undefined
): string {
  if (!ip) return '—';
  return port === null || port === undefined ? ip : `${ip}:${port}`;
}

const NODE_ROLE_BADGE_CLASSES: Record<string, string> = {
  validator: 'badge badge-transfer',
  observer: 'badge badge-mint',
  peer: 'badge badge-neutral',
};

export function getNodeRoleBadgeClass(role: string): string {
  return NODE_ROLE_BADGE_CLASSES[role] ?? 'badge badge-neutral';
}

export function getNodeRoleLabel(role: string): string {
  return role.charAt(0).toUpperCase() + role.slice(1);
}

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
