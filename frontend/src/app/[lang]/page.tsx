'use client';

import { useCallback, useEffect, useState } from 'react';
import { StatsCard, StatsCardSkeleton, StatsRow } from '@/components/StatsCard';
import { BlocksTable, TransactionsTable } from '@/components/Tables';
import { Hash } from '@/components/Hash';
import { LiveIndicator, SectionHeading } from '@/components/States';
import { useLatestBlocks, useLatestTransactions, usePerps, useRandPrice, useStats, useSupply } from '@/hooks/useApi';
import { useNewBlocks, useNewTransactions, useStatsUpdates } from '@/hooks/useWebSocket';
import { TOKEN_SYMBOL } from '@/lib/utils';
import { L, useFmt, useT } from '@/i18n/client';
import type { BlockSummary, NetworkStats, TransactionSummary } from '@/types';

const LATEST_LIMIT = 10;

export default function DashboardPage() {
  const { data: fetchedStats, isLoading: statsLoading } = useStats();
  const { data: supply } = useSupply();
  const { data: perps } = usePerps();
  const { data: price } = useRandPrice();
  const { t, tp, rich } = useT();
  const fmt = useFmt();
  const { data: fetchedBlocks, isLoading: blocksLoading } = useLatestBlocks(LATEST_LIMIT);
  const { data: fetchedTxs, isLoading: txsLoading } = useLatestTransactions(LATEST_LIMIT);

  const [liveStats, setLiveStats] = useState<NetworkStats | null>(null);
  const [liveBlocks, setLiveBlocks] = useState<BlockSummary[]>([]);
  const [liveTxs, setLiveTxs] = useState<TransactionSummary[]>([]);

  // Whenever a fresh REST payload lands, drop the locally accumulated deltas.
  useEffect(() => {
    if (fetchedBlocks) setLiveBlocks([]);
  }, [fetchedBlocks]);

  useEffect(() => {
    if (fetchedTxs) setLiveTxs([]);
  }, [fetchedTxs]);

  const onNewBlock = useCallback((block: BlockSummary) => {
    setLiveBlocks((current) =>
      current.some((b) => b.hash === block.hash) ? current : [block, ...current].slice(0, LATEST_LIMIT)
    );
  }, []);

  const onNewTransaction = useCallback((tx: TransactionSummary) => {
    setLiveTxs((current) =>
      current.some((t) => t.hash === tx.hash) ? current : [tx, ...current].slice(0, LATEST_LIMIT)
    );
  }, []);

  const { isConnected } = useNewBlocks(onNewBlock);
  useNewTransactions(onNewTransaction);
  useStatsUpdates(setLiveStats);

  const stats = liveStats ?? fetchedStats;

  const blocks = dedupe(
    [...liveBlocks, ...(fetchedBlocks ?? [])],
    (block) => block.hash
  ).slice(0, LATEST_LIMIT);

  const transactions = dedupe(
    [...liveTxs, ...(fetchedTxs ?? [])],
    (tx) => tx.hash
  ).slice(0, LATEST_LIMIT);

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-4xl font-semibold tracking-tight text-strong">
            {t('home.title')}
          </h1>
          <p className="mt-2 text-sm text-soft">
            {stats
              ? t('home.chain', { id: stats.chain_id })
              : t('home.connecting')}
          </p>
        </div>
        <LiveIndicator isConnected={isConnected} />
      </div>

      {/* Primary stats */}
      <StatsRow columns={4}>
        {statsLoading && !stats ? (
          Array.from({ length: 4 }).map((_, i) => <StatsCardSkeleton key={i} />)
        ) : (
          <>
            <StatsCard
              title={t('home.blockHeight')}
              value={fmt.number(stats?.height ?? 0)}
              subtitle={stats?.node_syncing ? t('home.nodeSyncing') : t('home.inSync')}
            />
            <StatsCard
              title={t('home.validators')}
              value={
                stats
                  ? `${fmt.number(stats.active_validator_count)} / ${fmt.number(stats.validator_count)}`
                  : '0'
              }
              subtitle={stats ? fmt.stake(stats.total_stake, t('home.activeStake')) : undefined}
            />
            <StatsCard
              title={t('home.transactions')}
              value={fmt.number(stats?.total_transactions ?? 0)}
              subtitle={stats ? tp('home.programs', stats.program_count) : undefined}
            />
            <StatsCard
              title={t('home.notes')}
              value={fmt.number(stats?.notes ?? 0)}
              subtitle={stats ? t('home.spent', { count: fmt.number(stats.nullifiers) }) : undefined}
            />
          </>
        )}
      </StatsRow>

      {/* Secondary stats */}
      <StatsRow columns={5}>
        <MiniStat label={t('home.avgBlockTime')} value={stats ? fmt.duration(stats.avg_block_time_ms) : '—'} />
        <MiniStat label={t('home.peers')} value={stats ? fmt.number(stats.peer_count) : '—'} />
        <MiniStat label={t('home.mempool')} value={stats ? fmt.number(stats.mempool_size) : '—'} />
        <MiniStat
          label={t('home.epoch')}
          value={
            stats && stats.epoch !== null
              ? tp('home.epochValue', stats.epoch_blocks ?? 0, { epoch: fmt.number(stats.epoch) })
              : '—'
          }
        />
        <MiniStat
          label={t('home.currentLeader')}
          value={
            stats?.current_leader ? (
              <Hash
                value={stats.current_leader}
                href={`/validators/${stats.current_leader}`}
                start={6}
                end={4}
              />
            ) : (
              '—'
            )
          }
        />
      </StatsRow>

      {stats && (
        <p className="flex flex-wrap items-center gap-x-2 text-xs text-mute">
          <span>
            {stats.total_supply !== '0'
              ? t('home.facts.totalSupply', { amount: fmt.amount(stats.total_supply) })
              : t('home.facts.supplyUnavailable')}
          </span>
          <span>· {t('home.facts.view', { view: fmt.number(stats.view) })}</span>
          <span className="inline-flex items-center gap-1">
            · {t('home.facts.treeRoot')}{' '}
            {stats.tree_root ? <Hash value={stats.tree_root} start={8} end={6} /> : '—'}
          </span>
          <span className="inline-flex items-center gap-1">
            · {t('home.facts.bundleGuest')}{' '}
            {stats.hc_bundle ? <Hash value={stats.hc_bundle} start={8} end={6} /> : '—'}
          </span>
          <span>· {stats.faucet ? t('home.facts.faucetOn') : t('home.facts.faucetOff')}</span>
          <span>· {stats.confidential ? t('home.facts.confidentialOn') : t('home.facts.confidentialOff')}</span>
          <span>
            · {tp('home.facts.decimals', stats.decimals, { symbol: TOKEN_SYMBOL })}
          </span>
          {stats.genesis_hash && (
            <span className="inline-flex items-center gap-1">
              · {t('home.facts.genesis')} <Hash value={stats.genesis_hash} start={8} end={6} />
            </span>
          )}
          {stats.node_version && (
            <span>
              ·{' '}
              {stats.node_git_sha
                ? t('home.facts.nodeSha', { version: stats.node_version, sha: stats.node_git_sha.slice(0, 7) })
                : t('home.facts.node', { version: stats.node_version })}
            </span>
          )}
        </p>
      )}

      {stats?.limits && (
        <p className="flex flex-wrap items-center gap-x-2 text-xs text-mute">
          <span>{tp('home.limits.programs', stats.limits.max_program_words)}</span>
          <span>· {t('home.limits.proofs', { size: fmt.binaryBytes(stats.limits.max_proof_bytes) })}</span>
          <span>· {t('home.limits.blocks', { size: fmt.binaryBytes(stats.limits.max_block_bytes) })}</span>
          <span>· {t('home.limits.callEnvelopes', { size: fmt.binaryBytes(stats.limits.max_call_envelope_bytes) })}</span>
          <span>
            ·{' '}
            {stats.limits.max_program_public_words > 0
              ? tp('home.limits.publicInput', stats.limits.max_program_public_words)
              : t('home.limits.publicInputOff')}
          </span>
          <span>
            ·{' '}
            {stats.limits.envelope_bytes
              ? t('home.limits.noteEnvelopesMemo', { bytes: fmt.number(stats.limits.envelope_bytes) })
              : t('home.limits.noteEnvelopesNoMemo', { bytes: fmt.number(1348) })}
          </span>
          <span>· {stats.limits.hardening_v6 ? t('home.limits.v6On') : t('home.limits.v6Off')}</span>
          <span className="inline-flex items-center gap-1">
            · {t('home.limits.authGuest')}{' '}
            {stats.limits.hc_auth ? <Hash value={stats.limits.hc_auth} start={8} end={6} /> : t('home.limits.none')}
          </span>
          <span>· {tp('home.limits.proofWindow', stats.limits.proof_window_blocks ?? 256)}</span>
          {stats.limits.binding_domain != null && (
            <span>
              ·{' '}
              {stats.limits.binding_domain === 1
                ? t('home.limits.bindingsGenesis')
                : t('home.limits.bindingsChainId')}
            </span>
          )}
        </p>
      )}

      {stats?.limits && (stats.limits.testnet || stats.limits.admission_by_vote || stats.limits.slashing) && (
        <p className="flex flex-wrap items-center gap-x-2 text-xs text-mute">
          <span>{stats.limits.testnet ? t('home.network.testnet') : t('home.network.mainnet')}</span>
          {stats.limits.admission_by_vote && (
            <span>
              · {rich('home.network.admission')}
            </span>
          )}
          <span>
            ·{' '}
            {stats.limits.slashing
              ? tp('home.network.slashing', stats.limits.slashing.jail_epochs, {
                  percent: fmt.percentage(stats.limits.slashing.equivocation_bps / 100),
                })
              : t('home.network.slashingOff')}
          </span>
        </p>
      )}

      {stats?.limits && stats.limits.gas_metering && (
        <p className="flex flex-wrap items-center gap-x-2 text-xs text-mute">
          <span>
            {stats.limits.gas_metering === 'circuit'
              ? t('home.gas.circuit')
              : t('home.gas.policy')}
          </span>
          <span>
            · {t('home.gas.perGas', { amount: fmt.amount((stats.gas_prices ?? stats.limits).gas_price ?? '0') })}
          </span>
          <span>
            · {t('home.gas.perKiB', { amount: fmt.amount((stats.gas_prices ?? stats.limits).byte_price ?? '0') })}
          </span>
          {(stats.limits.max_gas_price || stats.limits.max_byte_price) && (
            <span>
              ·{' '}
              {t(stats.limits.byte_load === 'paying' ? 'home.gas.ceilingsPaying' : 'home.gas.ceilings', {
                gas: fmt.amount(stats.limits.max_gas_price ?? '0'),
                byte: fmt.amount(stats.limits.max_byte_price ?? '0'),
              })}
            </span>
          )}
          {stats.limits.bundle_gas_limit !== null && (
            <span>· {t('home.gas.bundleLimit', { gas: fmt.number(stats.limits.bundle_gas_limit) })}</span>
          )}
          <span>
            ·{' '}
            {stats.limits.adjust_bps !== null
              ? t('home.gas.pricesMove', {
                  percent: (stats.limits.adjust_bps / 100).toFixed(2).replace(/\.?0+$/, ''),
                })
              : t('home.gas.pricesFixed')}
          </span>
          {stats.limits.program_state && (
            <span>
              ·{' '}
              {t('home.gas.programState', {
                fee: fmt.amount(stats.limits.program_state.cell_fee),
                reads: fmt.number(stats.limits.program_state.max_reads),
                writes: fmt.number(stats.limits.program_state.max_writes),
                payouts: fmt.number(stats.limits.program_state.max_payouts),
              })}
            </span>
          )}
        </p>
      )}

      {/* Fullnode v0.8.0's genesis sections: each line only on a chain that carries it. */}
      {stats?.limits && (stats.limits.fee_rules || stats.limits.multisig || stats.limits.perps) && (
        <p className="flex flex-wrap items-center gap-x-2 text-xs text-mute">
          {stats.limits.fee_rules && (
            <span>
              {t('home.v080.fees')}{' '}
              {[
                stats.limits.fee_rules.burn_floor
                  ? t('home.v080.burnFloor')
                  : stats.limits.fee_rules.burn_base
                    ? t('home.v080.burnBase')
                    : null,
                stats.limits.fee_rules.proposer_share_bps != null
                  ? t('home.v080.proposerShare', {
                      percent: fmt.percentage(stats.limits.fee_rules.proposer_share_bps / 100),
                    })
                  : null,
                stats.limits.fee_rules.prove_base
                  ? t('home.v080.proveBase', { amount: fmt.amount(stats.limits.fee_rules.prove_base) })
                  : null,
                stats.limits.fee_rules.usd_subsidy
                  ? t('home.v080.usdSubsidy', {
                      usd: fmt.units(stats.limits.fee_rules.usd_subsidy.usd_micros_per_sealed_block, 6),
                      cap: fmt.amount(stats.limits.fee_rules.usd_subsidy.max_subsidy_per_block),
                    })
                  : null,
              ]
                .filter(Boolean)
                .join(' · ')}
            </span>
          )}
          {price && (
            <span>
              ·{' '}
              {t('home.v080.price', { usd: fmt.units(price.price_micros_per_rand, 6) })}{' '}
              ({price.fresh ? t('tx.price.fresh') : t('tx.price.stale')})
            </span>
          )}
          {stats.limits.multisig && (
            <span>
              ·{' '}
              {t('home.v080.multisig', {
                fee: fmt.amount(stats.limits.multisig.create_fee),
                signers: fmt.number(stats.limits.multisig.max_signers),
                payouts: fmt.number(stats.limits.multisig.max_payouts),
              })}
            </span>
          )}
          {stats.limits.perps && (
            <span>
              ·{' '}
              {t('home.v080.perps', {
                markets: perps?.markets.map((m) => m.symbol).join(', ') || '—',
                window: fmt.number(stats.limits.perps.max_window_blocks),
                tier: fmt.number(stats.limits.perps.max_tier),
              })}
              {perps?.proved_height != null && (
                <> · {t('home.v080.perpsProved', { height: fmt.number(perps.proved_height) })}</>
              )}
            </span>
          )}
          {supply && (
            <>
              {stats.limits.fee_rules?.burn_base && supply.base_fees_burned != null && (
                <span>· {t('home.v080.baseFeesBurned', { amount: fmt.amount(supply.base_fees_burned) })}</span>
              )}
              {stats.limits.multisig && supply.multisig_rand_held != null && (
                <span>
                  ·{' '}
                  {t('home.v080.multisigHeld', {
                    amount: fmt.amount(supply.multisig_rand_held),
                    issued: fmt.amount(supply.multisig_issued ?? '0'),
                  })}
                </span>
              )}
              {stats.limits.perps && stats.limits.perps.collateral_asset === 0 && supply.perps_rand_held != null && (
                <span>· {t('home.v080.perpsHeld', { amount: fmt.amount(supply.perps_rand_held) })}</span>
              )}
            </>
          )}
        </p>
      )}

      {/* Latest blocks + transactions: side by side they show one-line rows and the columns that
          fit half the page (View, Fee and Height are on /blocks and /transactions), so the two
          cards line up row for row (one row height) and end at the same height. */}
      <div className="grid gap-6 xl:grid-cols-2">
        <section className="flex min-w-0 flex-col gap-4">
          <SectionHeading
            label={t('home.latestBlocks')}
            actions={
              <L href="/blocks" className="link text-sm">
                {t('home.viewAll')}
              </L>
            }
          />
          <BlocksTable
            blocks={blocks}
            isLoading={blocksLoading && blocks.length === 0}
            hideColumns={['view']}
            compact
            className="flex-1 [&_td]:h-[52px] [&_td]:px-3 [&_th]:px-3"
          />
        </section>

        <section className="flex min-w-0 flex-col gap-4">
          <SectionHeading
            label={t('home.latestTransactions')}
            actions={
              <L href="/transactions" className="link text-sm">
                {t('home.viewAll')}
              </L>
            }
          />
          <TransactionsTable
            transactions={transactions}
            isLoading={txsLoading && transactions.length === 0}
            hideColumns={['fee', 'height']}
            compact
            className="flex-1 [&_td]:h-[52px] [&_td]:px-3 [&_th]:px-3"
          />
        </section>
      </div>
    </div>
  );
}

function MiniStat({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="stat">
      <div className="truncate text-lg font-medium text-strong">{value}</div>
      <p className="stat-label">{label}</p>
    </div>
  );
}

function dedupe<T>(items: T[], key: (item: T) => string): T[] {
  const seen = new Set<string>();
  const out: T[] = [];
  for (const item of items) {
    const k = key(item);
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(item);
  }
  return out;
}
