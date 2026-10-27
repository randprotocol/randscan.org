'use client';

import { useState } from 'react';
import { Column, DataTable } from '@/components/DataTable';
import { Hash } from '@/components/Hash';
import { KeyPanel, type SubmittedKey } from '@/components/KeyPanel';
import { RoleBadge, SpentCell, type SpentState } from '@/components/OpenedNotes';
import { StatsCard, StatsRow } from '@/components/StatsCard';
import { PageHeader } from '@/components/States';
import { useTokens } from '@/hooks/useApi';
import * as api from '@/lib/api';
import { matchSpent } from '@/lib/spent';
import { tokenBalances, type TokenBalance } from '@/lib/utils';
import { L, useFmt, useT, type Fmt } from '@/i18n/client';
import { keyInfo, openNote, rebuildNote, type KeyInfo, type OpenedNote } from '@/lib/viewing';
import type { TokenInfo } from '@/types';

interface HistoryRow {
  leaf_index: number;
  cm: string;
  height: number;
  tx_hash: string | null;
  note: OpenedNote;
  spent: SpentState;
}

/** `tokens` resolves a received/sent note's `asset` index to its symbol and decimals — the same
 * registry a wallet's own `resolve_asset` reads through, so a zUSD note renders "12.50 zUSD"
 * rather than a bare unit count. */
function buildColumns(
  tokens: TokenInfo[] | undefined,
  t: (key: string) => string,
  fmt: Fmt,
): Column<HistoryRow>[] {
  return [
    {
      key: 'leaf',
      header: t('viewing.leaf'),
      render: (r) => (
        <L href={`/notes/${r.cm}`} prefetch={false} className="link font-mono">
          #{fmt.number(r.leaf_index)}
        </L>
      ),
    },
    {
      key: 'height',
      header: t('viewing.height'),
      render: (r) => (
        <L href={`/blocks/${r.height}`} prefetch={false} className="link font-mono">
          {fmt.number(r.height)}
        </L>
      ),
    },
    {
      key: 'tx',
      header: t('viewing.transaction'),
      render: (r) =>
        r.tx_hash ? <Hash value={r.tx_hash} href={`/transactions/${r.tx_hash}`} prefetch={false} /> : <span className="text-mute">{r.height === 0 ? t('viewing.genesis') : t('viewing.deposit')}</span>,
    },
    { key: 'role', header: t('viewing.role'), render: (r) => <RoleBadge role={r.note.role} /> },
    {
      key: 'amount',
      header: t('viewing.amount'),
      render: (r) => <span className="font-mono text-text">{fmt.tokenAmount(r.note.amount, r.note.asset, tokens)}</span>,
    },
    {
      key: 'counterparty',
      header: t('viewing.counterparty'),
      render: (r) => <Hash value={r.note.role === 'received' ? r.note.from : r.note.pk} start={8} end={6} />,
    },
    {
      key: 'status',
      header: t('viewing.status'),
      render: (r) => (r.note.role === 'received' ? <SpentCell spent={r.spent} /> : <span className="text-mute">—</span>),
    },
  ];
}

function buildBalanceColumns(
  tokens: TokenInfo[] | undefined,
  t: (key: string, vars?: Record<string, string | number>) => string,
  fmt: Fmt,
): Column<TokenBalance>[] {
  return [
    {
      key: 'token',
      header: t('viewing.token'),
      render: (b) =>
        b.asset === 0 ? (
          <span className="text-text">RAND</span>
        ) : b.token ? (
          <L href={`/tokens/${b.token.id_text}`} prefetch={false} className="link">
            {b.token.symbol} <span className="text-mute">{b.token.name}</span>
          </L>
        ) : (
          <span className="text-mute">{t('viewing.assetIndex', { index: b.asset })}</span>
        ),
    },
    {
      key: 'balance',
      header: t('viewing.spendable'),
      render: (b) => <span className="font-mono text-text">{fmt.tokenAmount(b.units.toString(), b.asset, tokens)}</span>,
    },
    { key: 'notes', header: t('viewing.unspentNotes'), render: (b) => <span className="font-mono">{fmt.number(b.notes)}</span> },
  ];
}

export default function ViewingPage() {
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ scanned: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<KeyInfo | null>(null);
  const [rows, setRows] = useState<HistoryRow[] | null>(null);
  const { data: tokenList } = useTokens();
  const { t, tp } = useT();
  const fmt = useFmt();
  const columns = buildColumns(tokenList?.tokens, t, fmt);

  const run = async ({ kind, key }: SubmittedKey) => {
    setBusy(true);
    setError(null);
    setRows(null);
    try {
      const ki = await keyInfo(key, kind);
      setInfo(ki);
      const found: HistoryRow[] = [];
      let from = 0;
      let scanned = 0;
      for (;;) {
        const page = await api.getEnvelopePage(from, 1000);
        for (const n of page.notes) {
          scanned += 1;
          // An envelope first; a chain-computed note (a bridge fee note has no envelope; a
          // deposit's may be junk) is rebuilt from its public opening for this key's own pk.
          let opened = n.envelope ? await openNote(n.cm, n.envelope, kind, key) : null;
          if (!opened && n.public) opened = await rebuildNote(n.cm, n.public, kind, key);
          if (opened) {
            found.push({ leaf_index: n.leaf_index, cm: n.cm, height: n.height, tx_hash: n.tx_hash, note: opened, spent: { state: 'unknown' } });
          }
        }
        setProgress({ scanned, total: page.total_leaves });
        if (page.next_leaf === null) break;
        from = page.next_leaf;
      }
      // Spent or not: the public nullifier set is downloaded and matched here. No nullifier of
      // this key is ever sent to the explorer. A download that fails fails the scan, since a note
      // of unknown status would silently drop out of the balances.
      const owned = found.filter((r) => r.note.role === 'received' && r.note.nullifier);
      const spent = await matchSpent(owned.map((r) => r.note.nullifier!), api.getNullifierPage);
      for (const r of owned) {
        const nf = spent.get(r.note.nullifier!);
        r.spent = nf ? { state: 'spent', nullifier: nf } : { state: 'unspent' };
      }
      setRows(found.sort((a, b) => b.leaf_index - a.leaf_index));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const received = rows?.filter((r) => r.note.role === 'received') ?? [];
  const balances = tokenBalances(
    received.filter((r) => r.spent.state === 'unspent').map((r) => r.note),
    tokenList?.tokens
  );
  const balance = balances[0].units;

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('viewing.title')}
        subtitle={t('viewing.subtitle')}
      />

      <KeyPanel kinds={['viewing']} onOpen={run} busy={busy} title={t('viewing.scanTitle')}>
        {progress && (
          <p className="border-t border-border-soft py-3 text-sm text-mute">
            {tp('viewing.scanned', progress.total, { scanned: fmt.number(progress.scanned) })}
            {busy ? '…' : ''}
          </p>
        )}
        {error && <p className="border-t border-border-soft py-3 text-sm text-negative">{error}</p>}
      </KeyPanel>

      {info && rows && (
        <>
          <StatsRow columns={4}>
            <StatsCard
              title={t('viewing.spendable')}
              value={fmt.amount(balance.toString())}
              subtitle={t('viewing.spendableHelp')}
            />
            <StatsCard
              title={t('viewing.notesReceived')}
              value={fmt.number(received.length)}
              subtitle={t('viewing.unspentCount', {
                count: fmt.number(received.filter((r) => r.spent.state === 'unspent').length),
              })}
            />
            <StatsCard
              title={t('viewing.notesSent')}
              value={fmt.number(rows.filter((r) => r.note.role === 'sent').length)}
            />
            <StatsCard
              title={t('viewing.address')}
              value={info.address ? `${info.address.slice(0, 14)}…${info.address.slice(-6)}` : '—'}
              subtitle={info.pk ? t('viewing.pk', { pk: info.pk.slice(0, 12) }) : undefined}
            />
          </StatsRow>
          {info.address && (
            <p className="text-xs text-mute">
              {t('viewing.shieldedAddress')} <Hash value={info.address} start={20} end={10} />
            </p>
          )}
          <DataTable columns={buildBalanceColumns(tokenList?.tokens, t, fmt)} data={balances} keyExtractor={(b) => String(b.asset)} />
          <DataTable columns={columns} data={rows} keyExtractor={(r) => r.cm} emptyMessage={t('viewing.empty')} />
          <p className="text-xs text-mute">
            {t('viewing.footnote')}
          </p>
        </>
      )}
    </div>
  );
}
