'use client';

import { useState } from 'react';
import { KeyPanel, type SubmittedKey } from '@/components/KeyPanel';
import { OpenedCallBlock, OpenedNoteBlock, type OpenedNoteRow } from '@/components/OpenedNotes';
import { useTokens } from '@/hooks/useApi';
import { useT } from '@/i18n/client';
import * as api from '@/lib/api';
import { openCall, openNote, rebuildNote, type OpenedCall, type OpenedNote } from '@/lib/viewing';
import type { TransactionKind } from '@/types';

/** The "Open with a key" panel of a transaction page. */
export function TransactionOpener({ hash, kind }: { hash: string; kind: TransactionKind }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [rows, setRows] = useState<OpenedNoteRow[] | null>(null);
  const [call, setCall] = useState<OpenedCall | null | undefined>(undefined);
  const [hIn, setHIn] = useState<string | null>(null);
  const [hasCallEnvelope, setHasCallEnvelope] = useState(false);
  // Never fetched more than once per page (SWR-cached): resolves a disclosed note's `asset`
  // index to its symbol and decimals ("12.50 zUSD") — the browser never sends which token it is
  // asking about, since the whole registry is what a wallet reads through too.
  const { data: tokenList } = useTokens();
  const { t } = useT();

  const run = async ({ kind: keyKind, key }: SubmittedKey) => {
    setBusy(true);
    setError(null);
    try {
      const env = await api.getTransactionEnvelopes(hash);
      const out: OpenedNoteRow[] = [];
      for (const n of env.notes) {
        let opened: OpenedNote | null | undefined;
        if (!n.envelope) {
          opened = undefined;
        } else if (keyKind === 'call') {
          opened = null;
        } else {
          opened = await openNote(n.cm, n.envelope, keyKind, key);
        }
        // A chain-computed note is rebuilt from its public opening for a viewing key's own pk
        // (a bridge fee note has no envelope at all; a deposit's envelope may be junk).
        if (!opened && n.public && keyKind !== 'call' && keyKind !== 'tx') {
          opened = await rebuildNote(n.cm, n.public, keyKind, key);
        }
        const row: OpenedNoteRow = { leaf_index: n.leaf_index, cm: n.cm, height: n.height, opened, public: n.public ?? null };
        // Spent state is not looked up here: asking the explorer about this note's nullifier would
        // tell it whose note it is. The History page matches the public set in the browser.
        out.push(row);
      }
      setRows(out);
      setHIn(env.h_in);
      setHasCallEnvelope(!!env.call_envelope);
      if ((env.kind === 'call' || env.kind === 'invoke') && env.call_envelope && env.h_in) {
        const callKind = keyKind === 'tx' ? 'call' : keyKind;
        setCall(await openCall(env.h_in, env.call_envelope, callKind, key));
      } else {
        setCall(undefined);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRows(null);
    } finally {
      setBusy(false);
    }
  };

  // A transaction page takes only keys that open this transaction's own data: never a spend
  // key or the wallet key file. A transaction key is also tried as the call key (above).
  return (
    <KeyPanel kinds={['viewing', 'tx']} onOpen={run} busy={busy}>
      {error && <p className="border-t border-border-soft py-3 text-sm text-negative">{error}</p>}
      {rows && rows.length === 0 && (
        <p className="border-t border-border-soft py-3 text-sm text-mute">
          {t('components.txOpener.noNotes')}
        </p>
      )}
      {rows?.map((row) => (
        <OpenedNoteBlock key={row.cm} row={row} tokens={tokenList?.tokens} />
      ))}
      {rows && (kind === 'call' || kind === 'invoke') && (hasCallEnvelope ? <OpenedCallBlock call={call} hIn={hIn} /> : <OpenedCallBlock call={undefined} hIn={hIn} />)}
    </KeyPanel>
  );
}
