'use client';

import { Hash } from '@/components/Hash';
import { DetailRow } from '@/components/States';
import { L, useFmt, useT } from '@/i18n/client';
import type { OpenedCall, OpenedNote } from '@/lib/viewing';
import type { Nullifier, PublicNote, TokenInfo } from '@/types';

export type SpentState = { state: 'unspent' } | { state: 'spent'; nullifier: Nullifier } | { state: 'unknown' };

export interface OpenedNoteRow {
  leaf_index: number;
  cm: string;
  height: number;
  /** null: the key does not open it; undefined: no envelope indexed. */
  opened: OpenedNote | null | undefined;
  /** The leaf's public opening when the chain computed the note (see `NoteEnvelope.public`). */
  public?: PublicNote | null;
  spent?: SpentState;
}

export function RoleBadge({ role }: { role: OpenedNote['role'] }) {
  const { t } = useT();
  const cls = role === 'received' ? 'badge badge-mint' : role === 'sent' ? 'badge badge-transfer' : 'badge badge-neutral';
  const label = role === 'received'
      ? t('components.openedNotes.received')
      : role === 'sent'
        ? t('components.openedNotes.sent')
        : t('components.openedNotes.openedWithTxKey');
  return <span className={cls}>{label}</span>;
}

export function SpentCell({ spent }: { spent?: SpentState }) {
  const { t } = useT();
  const fmt = useFmt();
  if (!spent || spent.state === 'unknown') return <span className="text-mute">—</span>;
  if (spent.state === 'unspent') return <span className="text-accent">{t('components.openedNotes.unspent')}</span>;
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <span className="text-soft">{t('components.openedNotes.spentIn')}</span>
      <Hash value={spent.nullifier.tx_hash} href={`/transactions/${spent.nullifier.tx_hash}`} prefetch={false} start={8} end={6} />
      <span className="text-mute">(#{fmt.number(spent.nullifier.height)})</span>
    </span>
  );
}

/** One note of a transaction, as opened (or not) by the pasted key. `tokens` resolves the
 * disclosed `asset` index to a symbol and decimals ("12.50 zUSD"); a dummy input or output —
 * sealed to nobody — opens for no key, so it renders the same as "not opened by this key". */
export function OpenedNoteBlock({ row, tokens }: { row: OpenedNoteRow; tokens?: TokenInfo[] }) {
  const { t } = useT();
  const fmt = useFmt();
  const title = (
    <span className="inline-flex flex-wrap items-center gap-2">
      <L href={`/notes/${row.cm}`} prefetch={false} className="link font-mono">
        {t('components.openedNotes.leaf', { n: fmt.number(row.leaf_index) })}
      </L>
      <Hash value={row.cm} start={10} end={6} />
    </span>
  );
  const feeHint =
    row.public?.source === 'bridge_fee'
      ? t('components.openedNotes.feeHint')
      : null;
  if (row.opened === undefined) {
    return (
      <div className="border-t border-border-soft py-3">
        {title}
        <p className="mt-1 text-sm text-mute">{feeHint ?? t('components.openedNotes.notIndexed')}</p>
      </div>
    );
  }
  if (row.opened === null) {
    return (
      <div className="border-t border-border-soft py-3">
        {title}
        <p className="mt-1 text-sm text-mute">{feeHint ?? t('components.openedNotes.notOpened')}</p>
      </div>
    );
  }
  const n = row.opened;
  return (
    <div className="border-t border-border-soft py-2">
      <div className="flex flex-wrap items-center justify-between gap-2 py-2">
        {title}
        <RoleBadge role={n.role} />
      </div>
      <DetailRow label={t('components.openedNotes.amount')}>
        <span className="text-base font-semibold text-strong">{fmt.tokenAmount(n.amount, n.asset, tokens)}</span>
      </DetailRow>
      <DetailRow label={t('components.openedNotes.owner')}>
        <Hash value={n.pk} full />
      </DetailRow>
      <DetailRow label={t('components.openedNotes.createdBy')}>
        <Hash value={n.from} full />
      </DetailRow>
      <DetailRow label={t('components.openedNotes.noteTime')}>
        <span className="font-mono">{fmt.number(n.time)}</span>
      </DetailRow>
      <DetailRow label={t('components.openedNotes.commitment')}>
        <span className="text-accent">
          {n.source
            ? t('components.openedNotes.verifiedRebuilt')
            : t('components.openedNotes.verifiedRecomputed')}
        </span>
      </DetailRow>
      {n.nullifier && (
        <>
          <DetailRow label={t('components.openedNotes.nullifier')}>
            <Hash value={n.nullifier} full />
          </DetailRow>
          <DetailRow label={t('components.openedNotes.status')}>
            <SpentCell spent={row.spent} />
          </DetailRow>
        </>
      )}
    </div>
  );
}

/** A call's opened input transcript. */
export function OpenedCallBlock({ call, hIn }: { call: OpenedCall | null | undefined; hIn: string | null }) {
  const { t } = useT();
  if (call === undefined) {
    return <p className="border-t border-border-soft py-3 text-sm text-mute">{t('components.openedNotes.noTranscript')}</p>;
  }
  if (call === null) {
    return <p className="border-t border-border-soft py-3 text-sm text-mute">{t('components.openedNotes.transcriptNotOpened')}</p>;
  }
  const role = call.role === 'caller'
      ? t('components.openedNotes.caller')
      : call.role === 'auditor'
        ? t('components.openedNotes.auditor')
        : t('components.openedNotes.openedWithCallKey');
  return (
    <div className="border-t border-border-soft py-2">
      <div className="flex flex-wrap items-center justify-between gap-2 py-2">
        <span className="text-sm text-strong">{t('components.openedNotes.callInputs')}</span>
        <span className="badge badge-call">{role}</span>
      </div>
      <DetailRow label={t('components.openedNotes.privateInputs')}>
        <span className="font-mono break-all">[{call.inputs.join(', ')}]</span>
      </DetailRow>
      <DetailRow label={t('components.openedNotes.salt')}>
        <span className="font-mono">[{call.salt.join(', ')}]</span>
      </DetailRow>
      <DetailRow label="H_IN">
        {call.faithful ? (
          <span className="text-accent">{t('components.openedNotes.faithful')} {hIn ? <Hash value={hIn} start={8} end={6} /> : null}</span>
        ) : (
          <span className="text-negative">{t('components.openedNotes.falseTranscript')}</span>
        )}
      </DetailRow>
    </div>
  );
}
