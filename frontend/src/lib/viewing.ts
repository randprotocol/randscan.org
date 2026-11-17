/**
 * Browser-side opening of shielded envelopes. The WebAssembly module (built from
 * crates/randscan-viewing, a re-implementation of the fullnode's open-side crypto) runs in the
 * page; a pasted key stays in React state and is never part of any request.
 */
import type { CallEnvelopeHex, EnvelopeHex, PublicNote } from '@/types';

/** How a pasted key is to be read. `file` is a wallet key file (`{"version":2,"spend_key":…}`). */
export type KeyKind = 'viewing' | 'spend' | 'file' | 'tx' | 'call';

export interface KeyInfo {
  kind: 'viewing' | 'tx' | 'call';
  nk: string | null;
  pk: string | null;
  address: string | null;
  from_spend_key: boolean;
}

export interface OpenedNote {
  /**
   * `received`: the note's owner is the key's own pk. `sent`: its creator is. `sent_unverified`:
   * the sender part opens under the key but the creator is a word the chain fixes (a relayed
   * deposit, a mint, a payout), so who sealed it cannot be checked — never counted as sent.
   * `tx_key`: opened with a transaction key, which says nothing about either.
   */
  role: 'received' | 'sent' | 'sent_unverified' | 'tx_key';
  tx_key: string;
  pk: string;
  from: string;
  /** Units, decimal string. */
  amount: string;
  asset: number;
  time: number;
  r: string;
  nullifier: string | null;
  verified: boolean;
  /** Set on a note rebuilt from its public opening (`rebuildNote`), not opened from an envelope. */
  source?: PublicNote['source'];
}

export interface OpenedCall {
  role: 'caller' | 'auditor' | 'call_key';
  call_key: string;
  salt: number[];
  inputs: number[];
  faithful: boolean;
}

interface WasmModule {
  key_info(input: string, kind: string): string;
  open_note(cm: string, envelopeJson: string, keyKind: string, key: string): string;
  open_call(hIn: string, envelopeJson: string, keyKind: string, key: string): string;
  nullifier_of(keyKind: string, key: string, cm: string): string;
  rebuild_note(cm: string, publicJson: string, keyKind: string, key: string): string;
}

let loading: Promise<WasmModule> | null = null;

/** Load (once) the WebAssembly opener. Browser only. */
export function loadViewing(): Promise<WasmModule> {
  if (typeof window === 'undefined') {
    return Promise.reject(new Error('viewing runs in the browser only'));
  }
  if (!loading) {
    loading = (async () => {
      // A plain URL string keeps the bundler out of it: the file is served from /public.
      const url = `${window.location.origin}/viewing/randscan_viewing.js`;
      const mod = (await import(/* webpackIgnore: true */ url)) as unknown as {
        default: (input?: string) => Promise<unknown>;
      } & WasmModule;
      await mod.default('/viewing/randscan_viewing_bg.wasm');
      return mod;
    })().catch((err) => {
      loading = null;
      throw err;
    });
  }
  return loading;
}

function parse<T>(json: string): T | null {
  return json === 'null' ? null : (JSON.parse(json) as T);
}

/** The wasm throws plain strings on malformed input; normalise to Error. */
function wrap<T>(f: () => T): T {
  try {
    return f();
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
  }
}

export async function keyInfo(input: string, kind: KeyKind): Promise<KeyInfo> {
  const m = await loadViewing();
  return wrap(() => JSON.parse(m.key_info(input, kind)) as KeyInfo);
}

export async function openNote(
  cm: string,
  envelope: EnvelopeHex,
  kind: KeyKind,
  key: string
): Promise<OpenedNote | null> {
  const m = await loadViewing();
  return wrap(() => parse<OpenedNote>(m.open_note(cm, JSON.stringify(envelope), kind, key)));
}

/**
 * Rebuild a chain-computed note for a viewing key from its public opening and accept it only if
 * it commits to the leaf `cm` with the key's own pk. Finds a bridge fee note (no envelope) and a
 * deposit or mint whose envelope is junk. A transaction or call key cannot rebuild: `null`.
 */
export async function rebuildNote(cm: string, pub: PublicNote, kind: KeyKind, key: string): Promise<OpenedNote | null> {
  if (kind === 'tx' || kind === 'call') return null;
  const m = await loadViewing();
  return wrap(() => parse<OpenedNote>(m.rebuild_note(cm, JSON.stringify(pub), kind, key)));
}

export async function openCall(
  hIn: string,
  envelope: CallEnvelopeHex,
  kind: KeyKind | 'auditor',
  key: string
): Promise<OpenedCall | null> {
  const m = await loadViewing();
  return wrap(() => parse<OpenedCall>(m.open_call(hIn, JSON.stringify(envelope), kind, key)));
}

export async function nullifierOf(kind: KeyKind, key: string, cm: string): Promise<string> {
  const m = await loadViewing();
  return wrap(() => m.nullifier_of(kind, key, cm));
}

/** A key file pastes as JSON; anything else is 64 hex characters whose meaning the user picks. */
export function detectKeyKind(input: string, chosen: KeyKind): KeyKind {
  const s = input.trim();
  if (s.startsWith('{')) return 'file';
  return chosen;
}

export function looksLikeKey(input: string, acceptsFile = true): boolean {
  const s = input.trim();
  if (s.startsWith('{')) return acceptsFile && /"spend_key"\s*:\s*"[0-9a-fA-F]{64}"/.test(s);
  return /^(0x)?[0-9a-fA-F]{64}$/.test(s);
}

const STORAGE_KEY = 'randscan.viewing-key';

/** Per-tab memory of a key, only when the user asked for it. */
export function rememberKey(kind: KeyKind, key: string): void {
  try {
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify({ kind, key }));
  } catch {
    // storage unavailable: nothing to do
  }
}

export function forgetKey(): void {
  try {
    sessionStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}

export function recallKey(): { kind: KeyKind; key: string } | null {
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const v = JSON.parse(raw) as { kind: KeyKind; key: string };
    return v && typeof v.key === 'string' ? v : null;
  } catch {
    return null;
  }
}
