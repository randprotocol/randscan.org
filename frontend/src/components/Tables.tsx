'use client';

import { Column, DataTable } from './DataTable';
import { Hash } from './Hash';
import { KindBadge } from './KindBadge';
import { useTokens } from '@/hooks/useApi';
import { L, useFmt, useT, type Fmt } from '@/i18n/client';
import type { BlockSummary, TokenInfo, TransactionSummary } from '@/types';

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

type TFn = ReturnType<typeof useT>['t'];

/** Hash lengths: `compact` is for a table sharing the row with another (the home page). */
const HASH = { start: 8, end: 6 };
const HASH_COMPACT = { start: 6, end: 4 };

export function blockColumns(t: TFn, fmt: Fmt, compact = false): Column<BlockSummary>[] {
  const h = compact ? HASH_COMPACT : HASH;
  return [
    {
      key: 'height',
      header: t('components.tables.blocks.height'),
      render: (block) => (
        <L href={`/blocks/${block.height}`} className="link font-mono">
          {fmt.number(block.height)}
        </L>
      ),
    },
    {
      key: 'hash',
      header: t('components.tables.blocks.hash'),
      render: (block) => <Hash value={block.hash} href={`/blocks/${block.hash}`} {...h} />,
    },
    {
      key: 'proposer',
      header: t('components.tables.blocks.proposer'),
      render: (block) => (
        <Hash
          value={block.proposer}
          href={`/validators/${block.proposer}`}
          start={compact ? 4 : 6}
          end={compact ? 4 : 6}
        />
      ),
    },
    {
      key: 'tx_count',
      header: t('components.tables.blocks.txs'),
      render: (block) => <span className="font-mono text-soft">{fmt.number(block.tx_count)}</span>,
    },
    {
      key: 'view',
      header: t('components.tables.blocks.view'),
      render: (block) => <span className="font-mono text-soft">{fmt.number(block.view)}</span>,
    },
    {
      key: 'timestamp_ms',
      header: t('components.tables.blocks.age'),
      render: (block) => (
        <span className="text-mute" title={String(block.timestamp_ms)}>
          {fmt.ago(block.timestamp_ms)}
        </span>
      ),
    },
  ];
}

interface BlocksTableProps {
  blocks: BlockSummary[];
  isLoading?: boolean;
  emptyMessage?: string;
  /** Drop columns that are redundant in the surrounding context. */
  hideColumns?: string[];
  /** Shorter hashes, for a table that shares the row with another. */
  compact?: boolean;
  className?: string;
}

export function BlocksTable({
  blocks,
  isLoading,
  emptyMessage,
  hideColumns = [],
  compact = false,
  className,
}: BlocksTableProps) {
  const { t } = useT();
  const fmt = useFmt();
  return (
    <DataTable
      columns={blockColumns(t, fmt, compact).filter((column) => !hideColumns.includes(column.key))}
      className={className}
      data={blocks}
      keyExtractor={(block) => block.hash}
      isLoading={isLoading}
      emptyMessage={emptyMessage ?? t('components.tables.blocks.empty')}
    />
  );
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

/**
 * What the action touched: the program of a deploy/call, the validator of a staking move or
 * mint, the public amount of a deposit or an RPL registration/mint/burn. A plain transfer shows
 * nothing, by design — its asset is private, and `tokens` (the cached registry) resolves an RPL
 * amount's index to its symbol ("12.50 zUSD") rather than a bare unit count.
 */
function actionCell(tx: TransactionSummary, t: TFn, fmt: Fmt, tokens?: TokenInfo[], compact = false) {
  const parts: React.ReactNode[] = [];
  const n = compact ? 4 : 6;
  if (tx.program) {
    parts.push(
      <Hash key="program" value={tx.program} href={`/programs/${tx.program}`} start={n} end={n} />
    );
  }
  if (tx.validator) {
    parts.push(
      <Hash
        key="validator"
        value={tx.validator}
        href={`/validators/${tx.validator}`}
        start={n}
        end={n}
      />
    );
  }
  if (tx.amount !== null) {
    parts.push(
      <span key="amount" className="font-mono text-text">
        {fmt.tokenAmount(tx.amount, tx.asset_index, tokens)}
      </span>
    );
  }
  if (parts.length === 0) {
    return (
      <span className="text-mute" title={t('components.tables.txs.hiddenTitle')}>
        {tx.kind === 'transfer' ? t('components.tables.txs.shielded') : '—'}
      </span>
    );
  }
  return <span className="inline-flex items-center gap-2">{parts}</span>;
}

export function transactionColumns(
  t: TFn,
  fmt: Fmt,
  tokens?: TokenInfo[],
  compact = false
): Column<TransactionSummary>[] {
  const h = compact ? HASH_COMPACT : HASH;
  return [
    {
      key: 'hash',
      header: t('components.tables.txs.hash'),
      render: (tx) => <Hash value={tx.hash} href={`/transactions/${tx.hash}`} {...h} />,
    },
    {
      key: 'kind',
      header: t('components.tables.txs.kind'),
      render: (tx) => <KindBadge kind={tx.kind} short />,
    },
    {
      key: 'height',
      header: t('components.tables.txs.height'),
      render: (tx) => (
        <L href={`/blocks/${tx.height}`} className="link font-mono">
          {fmt.number(tx.height)}
        </L>
      ),
    },
    {
      key: 'action',
      header: t('components.tables.txs.action'),
      render: (tx) => actionCell(tx, t, fmt, tokens, compact),
    },
    {
      key: 'fee',
      header: t('components.tables.txs.fee'),
      render: (tx) =>
        tx.has_bundle ? (
          <span className="font-mono text-mute">{fmt.amount(tx.fee)}</span>
        ) : (
          <span className="text-mute" title={t('components.tables.txs.noFeeTitle')}>
            —
          </span>
        ),
    },
    {
      key: 'timestamp_ms',
      header: t('components.tables.txs.age'),
      render: (tx) => (
        <span className="text-mute" title={String(tx.timestamp_ms)}>
          {fmt.ago(tx.timestamp_ms)}
        </span>
      ),
    },
  ];
}

interface TransactionsTableProps {
  transactions: TransactionSummary[];
  isLoading?: boolean;
  emptyMessage?: string;
  /** Drop columns that are redundant in the surrounding context. */
  hideColumns?: string[];
  /** Shorter hashes, for a table that shares the row with another. */
  compact?: boolean;
  className?: string;
}

export function TransactionsTable({
  transactions,
  isLoading,
  emptyMessage,
  hideColumns = [],
  compact = false,
  className,
}: TransactionsTableProps) {
  const { data: tokenList } = useTokens();
  const { t } = useT();
  const fmt = useFmt();
  const columns = transactionColumns(t, fmt, tokenList?.tokens, compact).filter((column) => !hideColumns.includes(column.key));

  return (
    <DataTable
      columns={columns}
      data={transactions}
      keyExtractor={(tx) => tx.hash}
      className={className}
      isLoading={isLoading}
      emptyMessage={emptyMessage ?? t('components.tables.txs.empty')}
    />
  );
}
