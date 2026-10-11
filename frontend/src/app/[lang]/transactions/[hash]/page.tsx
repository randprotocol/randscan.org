'use client';

import { useParams } from 'next/navigation';
import { Hash } from '@/components/Hash';
import { KindBadge } from '@/components/KindBadge';
import { TransactionOpener } from '@/components/TransactionOpener';
import { DetailSkeleton } from '@/components/Loading';
import { DetailRow, ErrorState, NotFoundState, PageHeader, Panel } from '@/components/States';
import { isNotFound, useMultisig, usePerps, useRandPrice, useTokens, useTransaction } from '@/hooks/useApi';
import { formatBridgeAddress, knownBridgeChainName, resolveToken } from '@/lib/utils';
import { L, useFmt, useT } from '@/i18n/client';
import type { BridgeFeeNote, Bundle, Payout, ProgramCell, Receipt, TokenInfo, TransactionDetail } from '@/types';

export default function TransactionDetailPage() {
  const params = useParams<{ hash: string }>();
  const hash = decodeURIComponent(params.hash);
  const { data: tx, error, isLoading, mutate } = useTransaction(hash);
  const { data: tokenList } = useTokens();
  const tokens = tokenList?.tokens ?? [];
  const { t } = useT();
  const fmt = useFmt();

  if (isLoading && !tx) {
    return <DetailSkeleton />;
  }

  if (error && isNotFound(error)) {
    return (
      <NotFoundState
        title={t('tx.notFound')}
        message={t('tx.notFoundMessage', { hash })}
        backHref="/transactions"
        backLabel={t('tx.back')}
      />
    );
  }

  if (error || !tx) {
    return <ErrorState message={t('tx.loadError')} onRetry={() => void mutate()} />;
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('tx.title')}
        subtitle={<Hash value={tx.hash} full copyable />}
        actions={<KindBadge kind={tx.kind} />}
      />

      <Panel title={t('tx.overview')}>
        <DetailRow label={t('tx.hash')}>
          <Hash value={tx.hash} full />
        </DetailRow>
        <DetailRow label={t('tx.kind')}>
          <KindBadge kind={tx.kind} />
        </DetailRow>
        <DetailRow label={t('tx.block')}>
          <span className="inline-flex flex-wrap items-center gap-2">
            <L href={`/blocks/${tx.height}`} className="link font-mono">
              #{fmt.number(tx.height)}
            </L>
            <span className="text-mute">·</span>
            <Hash value={tx.block_hash} href={`/blocks/${tx.block_hash}`} />
          </span>
        </DetailRow>
        <DetailRow label={t('tx.indexInBlock')}>
          <span className="font-mono">{fmt.number(tx.tx_index)}</span>
        </DetailRow>
        <DetailRow label={t('tx.timestamp')}>
          <span>
            {fmt.dateTime(tx.timestamp_ms)}{' '}
            <span className="text-mute">({fmt.ago(tx.timestamp_ms)})</span>
          </span>
        </DetailRow>
        <DetailRow label={t('tx.fee')}>
          {tx.has_bundle ? (
            fmt.amount(tx.fee)
          ) : (
            <span className="text-mute">{t('tx.noFee')}</span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.bundle')}>
          {tx.has_bundle ? (
            <span>{t('tx.bundleYes')}</span>
          ) : (
            <span className="inline-flex flex-wrap items-center gap-2">
              <span>{t('tx.bundleNo')}</span>
              {tx.validator ? (
                <Hash value={tx.validator} href={`/validators/${tx.validator}`} />
              ) : (
                <span className="text-mute">—</span>
              )}
            </span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.chainId')}>
          <span className="font-mono">{fmt.number(tx.chain_id)}</span>
        </DetailRow>
      </Panel>

      {tx.bundle && <BundlePanel bundle={tx.bundle} tokens={tokens} />}

      <KindPanel tx={tx} tokens={tokens} />

      {(tx.kind === 'call' || tx.kind === 'invoke') && <ReceiptPanel receipt={tx.receipt} />}

      {tx.kind !== 'other' && <TransactionOpener hash={tx.hash} kind={tx.kind} />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Invoke (RPL-2): a call plus the state transition it vouched for
// ---------------------------------------------------------------------------

function CellTable({ cells, written }: { cells: ProgramCell[]; written: boolean }) {
  const { t } = useT();
  const zero = '0'.repeat(64);
  return (
    <ul className="space-y-1.5">
      {cells.map((c) => (
        <li key={c.key} className="flex flex-col gap-0.5 text-xs">
          <span className="inline-flex items-center gap-2">
            <span className="w-10 flex-shrink-0 text-mute">{t('tx.cells.key')}</span>
            <Hash value={c.key} start={10} end={8} className="text-xs" />
          </span>
          <span className="inline-flex items-center gap-2">
            <span className="w-10 flex-shrink-0 text-mute">{written ? t('tx.cells.wrote') : t('tx.cells.read')}</span>
            {c.value === zero ? (
              <span className="text-mute">{written ? t('tx.cells.zerosDeleted') : t('tx.cells.zerosAbsent')}</span>
            ) : (
              <Hash value={c.value} start={10} end={8} className="text-xs" />
            )}
          </span>
        </li>
      ))}
    </ul>
  );
}

function PayoutList({ payouts, tokens }: { payouts: Payout[]; tokens: TokenInfo[] }) {
  const { t } = useT();
  const fmt = useFmt();
  return (
    <ul className="space-y-2">
      {payouts.map((p, i) => (
        <li key={p.cm} className="flex flex-col gap-0.5">
          <span>
            <span className="font-semibold text-strong">{fmt.tokenAmount(p.amount, p.asset, tokens)}</span>
            {p.asset !== 0 && (
              <span className="text-xs text-mute">
                {' '}· <AssetLink index={p.asset} tokens={tokens} />
              </span>
            )}
            <span className="text-xs text-mute"> · {t('tx.payout.nth', { n: i + 1 })}</span>
          </span>
          <span className="inline-flex items-center gap-2 text-xs">
            <span className="w-14 flex-shrink-0 text-mute">{t('tx.payout.to')}</span>
            <Hash value={p.recipient} start={12} end={8} className="text-xs" />
          </span>
          <span className="inline-flex items-center gap-2 text-xs">
            <span className="w-14 flex-shrink-0 text-mute">{t('tx.payout.note')}</span>
            <Hash value={p.cm} href={`/notes/${p.cm}`} start={10} end={8} className="text-xs" />
          </span>
        </li>
      ))}
    </ul>
  );
}

function InvokePanel({ tx, tokens, title }: { tx: TransactionDetail; tokens: TokenInfo[]; title: string }) {
  const { t } = useT();
  const fmt = useFmt();
  const tr = tx.transition ?? null;
  const bundle = tx.bundle;
  const inflow = tr?.inflow ?? 'none';
  return (
    <Panel title={title}>
      <DetailRow label={t('tx.program')}>
        {tx.program ? (
          <Hash value={tx.program} href={`/programs/${tx.program}`} full />
        ) : (
          <span className="text-mute">—</span>
        )}
      </DetailRow>
      <DetailRow label={t('tx.callProofSize')}>
        <span className="font-mono">{fmt.bytes(tx.call_proof_len)}</span>
      </DetailRow>
      <DetailRow label={t('tx.inputTranscript')}>
        {tx.input_envelope_len === null ? (
          <span className="text-mute">{t('tx.invoke.nonePublished')}</span>
        ) : (
          <span className="font-mono">{t('tx.sealed', { size: fmt.bytes(tx.input_envelope_len) })}</span>
        )}
      </DetailRow>
      <DetailRow label={t('tx.invoke.intoVault')}>
        {bundle && bundle.burn_r !== '0' ? (
          <span className="font-semibold text-strong">{fmt.amount(bundle.burn_r)}</span>
        ) : (
          <span className="text-mute">{t('tx.invoke.noRand')}</span>
        )}
        {bundle && bundle.burn_a !== '0' && (
          <span>
            {bundle.burn_r !== '0' ? ` ${t('tx.invoke.and')} ` : ''}
            <span className="font-semibold text-strong">
              {fmt.tokenAmount(bundle.burn_a, bundle.burn_asset, tokens)}
            </span>{' '}
            <span className="text-mute">
              {inflow === 'burn' ? t('tx.invoke.destroyed') : t('tx.invoke.deposited')}
            </span>
          </span>
        )}
      </DetailRow>
      <DetailRow label={t('tx.invoke.cellsRead', { count: fmt.number(tr?.reads.length ?? 0) })}>
        {tr && tr.reads.length > 0 ? <CellTable cells={tr.reads} written={false} /> : <span className="text-mute">{t('tx.invoke.none')}</span>}
      </DetailRow>
      <DetailRow label={t('tx.invoke.cellsWritten', { count: fmt.number(tr?.writes.length ?? 0) })}>
        {tr && tr.writes.length > 0 ? <CellTable cells={tr.writes} written /> : <span className="text-mute">{t('tx.invoke.none')}</span>}
      </DetailRow>
      <DetailRow label={t('tx.invoke.paidOut', { count: fmt.number(tr?.pays.length ?? 0) })}>
        {tr && tr.pays.length > 0 ? <PayoutList payouts={tr.pays} tokens={tokens} /> : <span className="text-mute">{t('tx.invoke.nothingLeft')}</span>}
      </DetailRow>
      <DetailRow label={t('tx.invoke.minted', { count: fmt.number(tr?.mints.length ?? 0) })}>
        {tr && tr.mints.length > 0 ? <PayoutList payouts={tr.mints} tokens={tokens} /> : <span className="text-mute">{t('tx.invoke.nothing')}</span>}
      </DetailRow>
      <p className="px-4 py-3 text-xs text-mute">{t('tx.invoke.help')}</p>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Bundle
// ---------------------------------------------------------------------------

function BundlePanel({ bundle, tokens }: { bundle: Bundle; tokens: TokenInfo[] }) {
  const { t } = useT();
  const fmt = useFmt();
  const burned = bundle.burn_a !== '0' || bundle.burn_r !== '0';
  return (
    <Panel title={t('tx.bundle')}>
      <DetailRow label={t('tx.bundlePanel.anchor')}>
        <Hash value={bundle.anchor} full />
      </DetailRow>
      {bundle.nullifiers.map((nf, i) => (
        <DetailRow key={`nf-${i}`} label={t(i < 2 ? 'tx.bundlePanel.nullifierAsset' : 'tx.bundlePanel.nullifierRand', { n: i + 1 })}>
          <Hash value={nf} full />
        </DetailRow>
      ))}
      {bundle.commitments.map((cm, i) => (
        <DetailRow key={`cm-${i}`} label={t(i < 2 ? 'tx.bundlePanel.commitmentAsset' : 'tx.bundlePanel.commitmentRand', { n: i + 1 })}>
          <Hash value={cm} href={`/notes/${cm}`} full />
        </DetailRow>
      ))}
      <DetailRow label={t('tx.fee')}>{fmt.amount(bundle.fee)}</DetailRow>
      <DetailRow label={t('tx.bundlePanel.burned')}>
        {!burned ? (
          <span className="text-mute">{t('tx.bundlePanel.nothingBurned')}</span>
        ) : (
          <span className="font-semibold text-strong">
            {bundle.burn_r !== '0' && <span>{fmt.amount(bundle.burn_r)} (RAND)</span>}
            {bundle.burn_a !== '0' && (
              <span>{fmt.tokenAmount(bundle.burn_a, bundle.burn_asset, tokens)}</span>
            )}
          </span>
        )}
      </DetailRow>
      <DetailRow label={t('tx.bundlePanel.time')}>
        <L href={`/blocks/${bundle.time}`} className="link font-mono">
          #{fmt.number(bundle.time)}
        </L>
      </DetailRow>
      <DetailRow label={t('tx.bundlePanel.proofSize')}>
        <span className="font-mono">{fmt.bytes(bundle.proof_len)}</span>
      </DetailRow>
      <DetailRow label={t('tx.bundlePanel.envelopeSizes')}>
        <span className="font-mono">
          {bundle.envelope_len.map((n) => fmt.bytes(n)).join(' · ')}
        </span>
      </DetailRow>
      {bundle.auth_proof_len > 0 && bundle.auth_commit ? (
        <>
          <DetailRow label={t('tx.bundlePanel.authCommit')}>
            <Hash value={bundle.auth_commit} full />
          </DetailRow>
          <DetailRow label={t('tx.bundlePanel.authProofSize')}>
            <span className="font-mono">{fmt.bytes(bundle.auth_proof_len)}</span>
          </DetailRow>
        </>
      ) : (
        <DetailRow label={t('tx.bundlePanel.authorisation')}>
          <span className="text-mute">{t('tx.bundlePanel.authInside')}</span>
        </DetailRow>
      )}
      <p className="px-4 py-3 text-xs text-mute">
        {t('tx.bundlePanel.help')}
        {bundle.auth_proof_len > 0 && ` ${t('tx.bundlePanel.splitAuth')}`}
      </p>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Kind-specific detail
// ---------------------------------------------------------------------------

function ValidatorRow({ label, address }: { label: string; address: string | null }) {
  return (
    <DetailRow label={label}>
      {address ? (
        <Hash value={address} href={`/validators/${address}`} full />
      ) : (
        <span className="text-mute">—</span>
      )}
    </DetailRow>
  );
}

function PqSignersRow({ signers }: { signers: number[] | null }) {
  const { t } = useT();
  if (!signers || signers.length === 0) return null;
  return (
    <DetailRow label={t('tx.pqCoSigners')}>
      <span className="font-mono">{signers.map((i) => `#${i}`).join(', ')}</span>
    </DetailRow>
  );
}

/** A resolved token's symbol as a link to its page, or "asset #N" when this build's cached
 * registry does not (yet) know the index. */
function FeeNoteRow({ note, tokens }: { note: BridgeFeeNote | null; tokens: TokenInfo[] }) {
  const { t } = useT();
  const fmt = useFmt();
  if (!note) return null;
  return (
    <DetailRow label={t('tx.bridgeFee')}>
      <span className="inline-flex flex-wrap items-center gap-2">
        <span>{fmt.tokenAmount(note.amount, note.asset, tokens)}</span>
        <span className="text-xs text-mute">{t('tx.feeNoteLeaf')}</span>
        <Hash value={note.commitment} href={`/notes/${note.commitment}`} start={10} end={6} />
      </span>
    </DetailRow>
  );
}

function AssetLink({ index, tokens }: { index: number | null; tokens: TokenInfo[] }) {
  const { t } = useT();
  if (index === null) return <span className="text-mute">—</span>;
  if (index === 0) return <span className="font-mono">RAND</span>;
  const token = resolveToken(tokens, index);
  if (!token) return <span className="font-mono">{t('tx.assetIndex', { index })}</span>;
  return (
    <L href={`/tokens/${token.id_text}`} className="link font-mono">
      {token.symbol} (#{index})
    </L>
  );
}

function KindPanel({ tx, tokens }: { tx: TransactionDetail; tokens: TokenInfo[] }) {
  const { t, tp, rich } = useT();
  const fmt = useFmt();
  const title = t('tx.kindDetails', { kind: t(`kinds.${tx.kind}.label`) });
  const chain = (id: number | null | undefined) =>
    fmt.bridgeChain(id, knownBridgeChainName(id));

  if (tx.kind === 'transfer') {
    return (
      <Panel title={title}>
        <p className="px-4 py-3 text-sm text-mute">{t('tx.transfer.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'mint') {
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.mint.depositNote')}>
          {tx.cm ? <Hash value={tx.cm} href={`/notes/${tx.cm}`} full /> : <span className="text-mute">—</span>}
        </DetailRow>
        <DetailRow label={t('tx.amount')}>
          <span className="text-base font-semibold text-strong">{fmt.amount(tx.amount)}</span>
        </DetailRow>
        <ValidatorRow label={t('tx.mint.mintedBy')} address={tx.validator} />
        <p className="px-4 py-3 text-xs text-mute">{t('tx.mint.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'deploy') {
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.program')}>
          {tx.program ? (
            <Hash value={tx.program} href={`/programs/${tx.program}`} full />
          ) : (
            <span className="text-mute">—</span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.deploy.wordsLength')}>
          <span className="font-mono">
            {tx.words_len === null ? '—' : tp('tx.deploy.words', tx.words_len)}
          </span>
        </DetailRow>
      </Panel>
    );
  }

  if (tx.kind === 'call') {
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.program')}>
          {tx.program ? (
            <Hash value={tx.program} href={`/programs/${tx.program}`} full />
          ) : (
            <span className="text-mute">—</span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.callProofSize')}>
          <span className="font-mono">{fmt.bytes(tx.call_proof_len)}</span>
        </DetailRow>
        <DetailRow label={t('tx.inputTranscript')}>
          {tx.input_envelope_len === null ? (
            <span className="text-mute">{t('tx.call.nonePublished')}</span>
          ) : (
            <span className="font-mono">{t('tx.sealed', { size: fmt.bytes(tx.input_envelope_len) })}</span>
          )}
        </DetailRow>
        <p className="px-4 py-3 text-xs text-mute">{t('tx.call.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'invoke') {
    return <InvokePanel tx={tx} tokens={tokens} title={title} />;
  }

  if (tx.kind === 'bond' || tx.kind === 'unbond' || tx.kind === 'withdraw') {
    return (
      <Panel title={title}>
        <ValidatorRow label={t('tx.staking.validator')} address={tx.validator} />
        <DetailRow label={t('tx.amount')}>
          <span className="text-base font-semibold text-strong">{fmt.amount(tx.amount)}</span>
        </DetailRow>
        {tx.kind === 'bond' && (
          <DetailRow label={t('tx.staking.registration')}>
            {tx.registered ? (
              <span className="badge badge-accent">{t('tx.staking.newValidator')}</span>
            ) : (
              <span className="text-mute">{t('tx.staking.existingValidator')}</span>
            )}
          </DetailRow>
        )}
        {tx.kind !== 'bond' && (
          <DetailRow label={t('tx.staking.registerNonce')}>
            <span className="font-mono">
              {tx.action_nonce === null ? '—' : fmt.number(tx.action_nonce)}
            </span>
          </DetailRow>
        )}
        {tx.kind === 'withdraw' && tx.note_time !== null && (
          <DetailRow label={t('tx.noteTime')}>
            <L href={`/blocks/${tx.note_time}`} className="link font-mono">
              #{fmt.number(tx.note_time)}
            </L>
          </DetailRow>
        )}
        <p className="px-4 py-3 text-xs text-mute">
          {tx.kind === 'bond'
            ? t('tx.staking.bondHelp')
            : tx.kind === 'unbond'
              ? t('tx.staking.unbondHelp')
              : t('tx.staking.withdrawHelp')}
        </p>
      </Panel>
    );
  }

  if (tx.kind === 'bridge_burn') {
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.asset')}>
          <AssetLink index={tx.asset_index} tokens={tokens} />
        </DetailRow>
        <DetailRow label={t('tx.amountBurned')}>
          <span className="text-base font-semibold text-strong">
            {fmt.tokenAmount(tx.amount, tx.asset_index, tokens)}
          </span>
        </DetailRow>
        {tx.release_amount != null && (
          <DetailRow label={t('tx.bridgeBurn.released')}>
            <span>{fmt.tokenAmount(tx.release_amount, tx.asset_index, tokens)}</span>
          </DetailRow>
        )}
        <FeeNoteRow note={tx.fee_note ?? null} tokens={tokens} />
        <DetailRow label={t('tx.bridgeBurn.relayerFee')}>
          <span>{fmt.tokenAmount(tx.relayer_fee, tx.asset_index, tokens)}</span>
        </DetailRow>
        <DetailRow label={t('tx.bridgeBurn.destChain')}>
          <span>{chain(tx.to_chain)}</span>
        </DetailRow>
        <DetailRow label={t('tx.bridgeBurn.destAddress')}>
          {tx.bridge_to ? (
            <span className="font-mono break-all" title={tx.bridge_to}>
              {formatBridgeAddress(tx.bridge_to)}
            </span>
          ) : (
            <span className="text-mute">—</span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.bridgeBurn.coinRedeemed')}>
          {tx.bridge_token ? (
            <span className="font-mono break-all" title={tx.bridge_token}>
              {formatBridgeAddress(tx.bridge_token)}
            </span>
          ) : (
            <span className="text-mute">—</span>
          )}
        </DetailRow>
        <p className="px-4 py-3 text-xs text-mute">{rich('tx.bridgeBurn.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'bridge_attest') {
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.attest.size')}>
          <span className="font-mono">{fmt.bytes(tx.attestation_len)}</span>
        </DetailRow>
        <DetailRow label={t('tx.recipient')}>
          {tx.recipient ? (
            <Hash value={tx.recipient} full />
          ) : (
            <span className="text-mute">—</span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.asset')}>
          <AssetLink index={tx.asset_index} tokens={tokens} />
        </DetailRow>
        <DetailRow label={t('tx.amount')}>
          {tx.amount === null ? (
            <span className="text-mute">{t('tx.attest.rotation')}</span>
          ) : (
            <span className="text-base font-semibold text-strong">
              {fmt.tokenAmount(tx.amount, tx.asset_index, tokens)}
              {tx.fee_note ? <span className="ms-2 text-xs font-normal text-mute">{t('tx.attest.gross')}</span> : null}
            </span>
          )}
        </DetailRow>
        {tx.fee_note && tx.deposit_amount != null && (
          <DetailRow label={t('tx.attest.deposited')}>
            <span>{fmt.tokenAmount(tx.deposit_amount, tx.asset_index, tokens)}</span>
          </DetailRow>
        )}
        <FeeNoteRow note={tx.fee_note ?? null} tokens={tokens} />
        <DetailRow label={t('tx.noteTime')}>
          {tx.note_time === null ? (
            <span className="text-mute">—</span>
          ) : (
            <L href={`/blocks/${tx.note_time}`} className="link font-mono">
              #{fmt.number(tx.note_time)}
            </L>
          )}
        </DetailRow>
        <DetailRow label={t('tx.attest.commitment')}>
          {tx.commitment ? (
            <Hash value={tx.commitment} href={`/notes/${tx.commitment}`} full />
          ) : (
            <span className="text-mute">{t('tx.attest.rotationShort')}</span>
          )}
        </DetailRow>
        <PqSignersRow signers={tx.pq_signers} />
        <p className="px-4 py-3 text-xs text-mute">
          {t('tx.attest.help')}
          {tx.fee_note ? ` ${t('tx.attest.feeHelp')}` : ''}
        </p>
      </Panel>
    );
  }

  if (tx.kind === 'register_token') {
    const action = tx.token_action?.kind === 'register_token' ? tx.token_action : null;
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.token')}>
          <L href={`/tokens/${(tx.asset_index !== null && resolveToken(tokens, tx.asset_index)?.id_text) || tx.asset_index}`} className="link">
            {action?.symbol ?? `#${tx.asset_index}`}
          </L>
        </DetailRow>
        <DetailRow label={t('tx.name')}>{action?.name ?? '—'}</DetailRow>
        <DetailRow label={t('tx.decimals')}>
          <span className="font-mono">{action ? fmt.number(action.decimals) : '—'}</span>
        </DetailRow>
        <DetailRow label={t('tx.authority')}>
          <span className="font-mono">{action?.authority ?? '—'}</span>
        </DetailRow>
        <DetailRow label={t('tx.registryIndex')}>
          <span className="font-mono">#{fmt.number(tx.asset_index)}</span>
        </DetailRow>
        <DetailRow label={t('tx.registerToken.initialMint')}>
          {action?.initial ? (
            <span className="text-base font-semibold text-strong">
              {t('tx.registerToken.amountTo', { amount: fmt.tokenAmount(action.initial.amount, tx.asset_index, tokens) })}{' '}
              <Hash value={action.initial.recipient} />
            </span>
          ) : (
            <span className="text-mute">{t('tx.none')}</span>
          )}
        </DetailRow>
        <p className="px-4 py-3 text-xs text-mute">{t('tx.registerToken.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'token_mint') {
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.asset')}>
          <AssetLink index={tx.asset_index} tokens={tokens} />
        </DetailRow>
        <DetailRow label={t('tx.amount')}>
          <span className="text-base font-semibold text-strong">
            {fmt.tokenAmount(tx.amount, tx.asset_index, tokens)}
          </span>
        </DetailRow>
        <DetailRow label={t('tx.recipient')}>
          {tx.recipient ? <Hash value={tx.recipient} full /> : <span className="text-mute">—</span>}
        </DetailRow>
        <DetailRow label={t('tx.mintNonce')}>
          <span className="font-mono">{tx.action_nonce === null ? '—' : fmt.number(tx.action_nonce)}</span>
        </DetailRow>
        <p className="px-4 py-3 text-xs text-mute">{t('tx.tokenMint.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'set_authority') {
    const action = tx.token_action?.kind === 'set_authority' ? tx.token_action : null;
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.asset')}>
          <AssetLink index={tx.asset_index} tokens={tokens} />
        </DetailRow>
        <DetailRow label={t('tx.nonce')}>
          <span className="font-mono">{tx.action_nonce === null ? '—' : fmt.number(tx.action_nonce)}</span>
        </DetailRow>
        <DetailRow label={t('tx.setAuthority.newAuthority')}>
          {action?.new_authority ? (
            <Hash value={action.new_authority} href={`/validators/${action.new_authority}`} full />
          ) : (
            <span className="text-mute">{t('tx.setAuthority.renounced')}</span>
          )}
        </DetailRow>
      </Panel>
    );
  }

  if (tx.kind === 'token_burn') {
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.asset')}>
          <AssetLink index={tx.asset_index} tokens={tokens} />
        </DetailRow>
        <DetailRow label={t('tx.amountBurned')}>
          <span className="text-base font-semibold text-strong">
            {fmt.tokenAmount(tx.amount, tx.asset_index, tokens)}
          </span>
        </DetailRow>
        <p className="px-4 py-3 text-xs text-mute">{t('tx.tokenBurn.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'pause_mints' || tx.kind === 'unpause_mints') {
    const action = tx.bridge_governance;
    const nonce = action && 'nonce' in action ? action.nonce : null;
    const signers = action && 'pq_signers' in action ? action.pq_signers : null;
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.nonce')}>
          <span className="font-mono">{nonce === null ? '—' : fmt.number(nonce)}</span>
        </DetailRow>
        <PqSignersRow signers={signers ?? null} />
        <p className="px-4 py-3 text-xs text-mute">
          {tx.kind === 'pause_mints'
            ? t('tx.pause.pauseHelp')
            : t('tx.pause.unpauseHelp')}
        </p>
      </Panel>
    );
  }

  if (tx.kind === 'register_bridged_token' || tx.kind === 'list_backing') {
    const action = tx.bridge_governance;
    return (
      <Panel title={title}>
        {action?.kind === 'register_bridged_token' && (
          <>
            <DetailRow label={t('tx.name')}>{action.name}</DetailRow>
            <DetailRow label={t('tx.symbol')}>{action.symbol}</DetailRow>
            <DetailRow label={t('tx.sourceChain')}>{chain(action.chain)}</DetailRow>
            <DetailRow label={t('tx.sourceToken')}>
              <span className="font-mono break-all">{formatBridgeAddress(action.token)}</span>
            </DetailRow>
            <DetailRow label={t('tx.sourceDecimals')}>
              <span className="font-mono">{fmt.number(action.decimals)}</span>
            </DetailRow>
          </>
        )}
        {action?.kind === 'list_backing' && (
          <>
            <DetailRow label={t('tx.token')}>
              <AssetLink index={action.token_index} tokens={tokens} />
            </DetailRow>
            <DetailRow label={t('tx.sourceChain')}>{chain(action.chain)}</DetailRow>
            <DetailRow label={t('tx.sourceToken')}>
              <span className="font-mono break-all">{formatBridgeAddress(action.token)}</span>
            </DetailRow>
            <DetailRow label={t('tx.sourceDecimals')}>
              <span className="font-mono">{fmt.number(action.decimals)}</span>
            </DetailRow>
          </>
        )}
        <PqSignersRow signers={tx.pq_signers} />
        <p className="px-4 py-3 text-xs text-mute">{t('tx.governance.help')}</p>
      </Panel>
    );
  }

  if (
    tx.kind === 'rotate_pq_guardians' ||
    tx.kind === 'rotate_pq_guardians_v2' ||
    tx.kind === 'rotate_pause_key' ||
    tx.kind === 'rotate_pause_key_v2' ||
    tx.kind === 'cancel_rotation'
  ) {
    const action = tx.bridge_governance;
    const delayed = tx.kind.endsWith('_v2');
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.rotation.nonce')}>
          <span className="font-mono">{tx.action_nonce === null ? '—' : fmt.number(tx.action_nonce)}</span>
        </DetailRow>
        {action && 'new_pq_guardians' in action && (
          <DetailRow label={t('tx.rotation.newSet')}>
            <span className="font-mono">{tp('tx.rotation.keys', action.new_pq_guardians.length)}</span>
          </DetailRow>
        )}
        {action && 'new_pause_key' in action && (
          <DetailRow label={t('tx.rotation.newPauseKey')}>
            <Hash value={action.new_pause_key} start={16} end={8} />
          </DetailRow>
        )}
        {action?.kind === 'rotate_pq_guardians_v2' && (
          <DetailRow label={t('tx.rotation.possession')}>
            <span className="font-mono">{fmt.number(action.possession_signatures)}</span>
          </DetailRow>
        )}
        {action?.kind === 'cancel_rotation' && (
          <DetailRow label={t('tx.rotation.cancelled')}>
            <span>{action.rotation_kind === 'pause_key' ? t('tx.rotation.pendingPauseKey') : t('tx.rotation.pendingGuardians')}</span>
          </DetailRow>
        )}
        {tx.kind !== 'cancel_rotation' && <PqSignersRow signers={tx.pq_signers} />}
        <p className="px-4 py-3 text-xs text-mute">
          {tx.kind === 'cancel_rotation'
            ? t('tx.rotation.cancelHelp')
            : delayed
              ? t('tx.rotation.delayedHelp')
              : t('tx.rotation.help')}
        </p>
      </Panel>
    );
  }

  if (tx.kind.startsWith('perp_')) {
    return <PerpPanel tx={tx} tokens={tokens} title={title} />;
  }

  if (tx.kind === 'create_multisig' || tx.kind.startsWith('multisig_')) {
    return <MultisigPanel tx={tx} tokens={tokens} title={title} />;
  }

  if (tx.kind === 'set_rand_price') {
    return <RandPricePanel tx={tx} title={title} />;
  }

  if (tx.kind === 'aggregate') {
    const a = tx.aggregate ?? null;
    return (
      <Panel title={title}>
        <DetailRow label={t('tx.aggregate.covers')}>
          <span className="font-mono">{a ? fmt.number(a.covers) : '—'}</span>
        </DetailRow>
        <DetailRow label={t('tx.aggregate.layout')}>
          {a?.layout === 'tree' ? (
            <span className="badge badge-accent">{t('tx.aggregate.tree')}</span>
          ) : a?.layout === 'flat' ? (
            <span>{t('tx.aggregate.flat')}</span>
          ) : (
            <span className="text-mute">—</span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.aggregate.payout')}>
          {a?.payout_total != null ? (
            <span className="text-base font-semibold text-strong">{fmt.amount(a.payout_total)}</span>
          ) : (
            <span className="text-mute">—</span>
          )}
        </DetailRow>
        <DetailRow label={t('tx.aggregate.aggregator')}>
          {a ? <Hash value={a.aggregator} full /> : <span className="text-mute">—</span>}
        </DetailRow>
        <DetailRow label={t('tx.aggregate.proofSize')}>
          <span className="font-mono">{a ? fmt.bytes(a.proof_len) : '—'}</span>
        </DetailRow>
        <DetailRow label={t('tx.nonce')}>
          <span className="font-mono">{a ? fmt.number(a.nonce) : '—'}</span>
        </DetailRow>
        <p className="px-4 py-3 text-xs text-mute">{t('tx.aggregate.help')}</p>
      </Panel>
    );
  }

  if (tx.kind === 'admit_validator' || tx.kind === 'slash_equivocation') {
    const action = tx.staking_action;
    return (
      <Panel title={title}>
        {action?.kind === 'admit_validator' && (
          <>
            <ValidatorRow label={t('tx.admit.candidate')} address={action.candidate} />
            <DetailRow label={t('tx.admit.voters')}>
              <span className="font-mono">{tp('tx.admit.validators', action.voters.length)}</span>
            </DetailRow>
            <DetailRow label={t('tx.admit.votedBy')}>
              <span className="flex flex-col gap-1">
                {action.voters.map((v) => (
                  <Hash key={v} value={v} href={`/validators/${v}`} start={10} end={6} />
                ))}
              </span>
            </DetailRow>
          </>
        )}
        {action?.kind === 'slash_equivocation' && (
          <>
            <ValidatorRow label={t('tx.slash.offender')} address={action.offender} />
            <DetailRow label={t('tx.slash.view')}>
              <span className="font-mono">{fmt.number(action.view)}</span>
            </DetailRow>
            <DetailRow label={t('tx.slash.firstHeader')}>
              <span className="font-mono">
                #{fmt.number(action.first.height)} <Hash value={action.first.hash} start={10} end={6} />
              </span>
            </DetailRow>
            <DetailRow label={t('tx.slash.secondHeader')}>
              <span className="font-mono">
                #{fmt.number(action.second.height)} <Hash value={action.second.hash} start={10} end={6} />
              </span>
            </DetailRow>
          </>
        )}
        <p className="px-4 py-3 text-xs text-mute">
          {tx.kind === 'admit_validator'
            ? t('tx.admit.help')
            : t('tx.slash.help')}
        </p>
      </Panel>
    );
  }

  // other
  return (
    <Panel title={title}>
      <p className="px-4 py-3 text-sm text-mute">{t('tx.unknownKind.help')}</p>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Fullnode v0.8.0: perps, multisig accounts, the RAND price vote
// ---------------------------------------------------------------------------

/** Signers named by their position in an account's (or the guardian set's) list. */
function Positions({ list }: { list: number[] }) {
  return <span className="font-mono">{list.length ? list.map((i) => `#${i}`).join(', ') : '—'}</span>;
}

/** A collateral amount: in the perps collateral asset's own units when the chain says which,
 * else the bare units. */
function useCollateral(tokens: TokenInfo[]) {
  const fmt = useFmt();
  const { data: perps } = usePerps();
  const asset = perps?.collateral_asset;
  return {
    perps,
    asset,
    amount: (units: string) => (asset === undefined ? fmt.units(units, 0) : fmt.tokenAmount(units, asset, tokens)),
  };
}

function PerpPanel({ tx, tokens, title }: { tx: TransactionDetail; tokens: TokenInfo[]; title: string }) {
  const { t } = useT();
  const fmt = useFmt();
  const { perps, asset, amount } = useCollateral(tokens);
  const a = tx.perp_action ?? null;
  const market = (id: number) => perps?.markets.find((m) => m.id === id)?.symbol ?? t('tx.perp.marketId', { id });
  const help =
    a?.kind === 'perp_deposit'
      ? t('tx.perp.depositHelp')
      : a?.kind === 'perp_order'
        ? t('tx.perp.orderHelp')
        : a?.kind === 'perp_cancel'
          ? t('tx.perp.cancelHelp')
          : a?.kind === 'perp_withdraw'
            ? t('tx.perp.withdrawHelp')
            : a?.kind === 'perp_oracle'
              ? t('tx.perp.oracleHelp')
              : t('tx.perp.stateProofHelp');
  const account = a && 'account' in a ? a.account : null;
  return (
    <Panel title={title}>
      {account && (
        <DetailRow label={t('tx.perp.account')}>
          <Hash value={account} full />
        </DetailRow>
      )}
      {a?.kind === 'perp_deposit' && (
        <>
          <DetailRow label={t('tx.perp.tradingKey')}>
            <Hash value={a.trading_key} start={16} end={8} />
          </DetailRow>
          <DetailRow label={t('tx.perp.deposited')}>
            {tx.bundle && tx.bundle.burn_a !== '0' ? (
              <span className="text-base font-semibold text-strong">
                {fmt.tokenAmount(tx.bundle.burn_a, tx.bundle.burn_asset, tokens)}
              </span>
            ) : tx.bundle && tx.bundle.burn_r !== '0' ? (
              <span className="text-base font-semibold text-strong">{fmt.amount(tx.bundle.burn_r)}</span>
            ) : (
              <span className="text-mute">—</span>
            )}
          </DetailRow>
        </>
      )}
      {a?.kind === 'perp_order' && (
        <>
          <DetailRow label={t('tx.perp.market')}>
            <span className="font-mono">{market(a.body.market)}</span>
          </DetailRow>
          <DetailRow label={t('tx.perp.side')}>
            <span className={a.body.side === 0 ? 'badge badge-mint' : 'badge badge-bridge'}>
              {a.body.side === 0 ? t('tx.perp.buy') : t('tx.perp.sell')}
            </span>
          </DetailRow>
          <DetailRow label={t('tx.perp.orderType')}>
            <span>
              {a.body.kind === 0 ? t('tx.perp.limit') : t('tx.perp.marketOrder')} ·{' '}
              {a.body.tif === 0 ? t('tx.perp.gtc') : a.body.tif === 1 ? t('tx.perp.ioc') : t('tx.perp.postOnly')}
              {a.body.reduce_only && <> · {t('tx.perp.reduceOnly')}</>}
            </span>
          </DetailRow>
          <DetailRow label={t('tx.perp.price')}>
            {a.body.kind === 1 ? (
              <span className="text-mute">{t('tx.perp.atMarket')}</span>
            ) : (
              <span className="font-semibold text-strong">{amount(a.body.price)}</span>
            )}
          </DetailRow>
          <DetailRow label={t('tx.perp.size')}>
            <span className="font-mono">{fmt.units(a.body.size, 0)}</span>
          </DetailRow>
          <DetailRow label={t('tx.perp.nonce')}>
            <span className="font-mono">{fmt.number(a.body.nonce)}</span>
          </DetailRow>
        </>
      )}
      {a?.kind === 'perp_cancel' && (
        <>
          <DetailRow label={t('tx.perp.cancels')}>
            <span>{t('tx.perp.cancelsValue', { nonce: fmt.number(a.target) })}</span>
          </DetailRow>
          <DetailRow label={t('tx.perp.nonce')}>
            <span className="font-mono">{fmt.number(a.nonce)}</span>
          </DetailRow>
        </>
      )}
      {a?.kind === 'perp_withdraw' && (
        <>
          <DetailRow label={t('tx.amount')}>
            <span className="text-base font-semibold text-strong">{amount(a.amount)}</span>
          </DetailRow>
          <DetailRow label={t('tx.perp.paidTo')}>
            <Hash value={a.recipient} start={12} end={8} />
          </DetailRow>
          <DetailRow label={t('tx.noteTime')}>
            <L href={`/blocks/${a.time}`} className="link font-mono">
              #{fmt.number(a.time)}
            </L>
          </DetailRow>
          <DetailRow label={t('tx.perp.nonce')}>
            <span className="font-mono">{fmt.number(a.nonce)}</span>
          </DetailRow>
        </>
      )}
      {a?.kind === 'perp_oracle' && (
        <>
          <ValidatorRow label={t('tx.staking.validator')} address={a.validator} />
          <DetailRow label={t('tx.perp.prices')}>
            <ul className="space-y-1">
              {a.prices.map((p) => (
                <li key={p.market}>
                  <span className="font-mono">{market(p.market)}</span>{' '}
                  <span className="font-semibold text-strong">{amount(p.price)}</span>
                </li>
              ))}
            </ul>
          </DetailRow>
          <DetailRow label={t('tx.nonce')}>
            <span className="font-mono">{fmt.number(a.nonce)}</span>
          </DetailRow>
        </>
      )}
      {a?.kind === 'perp_state_proof' && (
        <>
          <DetailRow label={t('tx.perp.window')}>
            <span className="inline-flex flex-wrap items-center gap-1 font-mono">
              <L href={`/blocks/${a.from_height + 1}`} className="link">
                #{fmt.number(a.from_height + 1)}
              </L>
              <span className="text-mute">→</span>
              <L href={`/blocks/${a.to_height}`} className="link">
                #{fmt.number(a.to_height)}
              </L>
            </span>
          </DetailRow>
          <DetailRow label={t('tx.perp.newRoot')}>
            <Hash value={a.new_root} full />
          </DetailRow>
          <DetailRow label={t('tx.perp.payouts')}>
            {a.payouts.length === 0 ? (
              <span className="text-mute">{t('tx.invoke.none')}</span>
            ) : (
              <ul className="space-y-1">
                {a.payouts.map((p) => (
                  <li key={p.request} className="inline-flex flex-wrap items-center gap-2">
                    <span className="font-semibold text-strong">{amount(p.amount)}</span>
                    <span className="text-xs text-mute">{t('tx.perp.request')}</span>
                    <Hash value={p.request} start={10} end={6} className="text-xs" />
                  </li>
                ))}
              </ul>
            )}
          </DetailRow>
          <DetailRow label={t('tx.perp.fees')}>
            <span>{amount(a.fees)}</span>
          </DetailRow>
          <DetailRow label={t('tx.perp.proofSize')}>
            <span className="font-mono">{fmt.bytes(a.proof_bytes)}</span>
          </DetailRow>
        </>
      )}
      {asset !== undefined && asset !== 0 && (
        <DetailRow label={t('tx.perp.collateral')}>
          <AssetLink index={asset} tokens={tokens} />
        </DetailRow>
      )}
      <p className="px-4 py-3 text-xs text-mute">{help}</p>
    </Panel>
  );
}

function MultisigPanel({ tx, tokens, title }: { tx: TransactionDetail; tokens: TokenInfo[]; title: string }) {
  const { t } = useT();
  const fmt = useFmt();
  const a = tx.multisig_action ?? null;
  const id = a && 'account' in a ? a.account : null;
  const { data: account } = useMultisig(id);
  const funded = tx.bundle && (tx.bundle.burn_r !== '0' || tx.bundle.burn_a !== '0');
  const help =
    a?.kind === 'create_multisig'
      ? t('tx.multisig.createHelp')
      : a?.kind === 'multisig_deposit'
        ? t('tx.multisig.depositHelp')
        : a?.kind === 'multisig_pay'
          ? t('tx.multisig.payHelp')
          : t('tx.multisig.rotateHelp');
  return (
    <Panel title={title}>
      {id && (
        <DetailRow label={t('tx.multisig.account')}>
          <Hash value={id} full />
        </DetailRow>
      )}
      {a?.kind === 'create_multisig' && (
        <>
          <DetailRow label={t('tx.multisig.threshold')}>
            <span className="font-mono">{t('tx.multisig.ofSigners', { m: fmt.number(a.threshold), n: fmt.number(a.signer_count) })}</span>
          </DetailRow>
          <DetailRow label={t('tx.multisig.salt')}>
            <Hash value={a.salt} start={10} end={6} />
          </DetailRow>
        </>
      )}
      {(a?.kind === 'create_multisig' || a?.kind === 'multisig_deposit') && (
        <DetailRow label={t('tx.multisig.funding')}>
          {funded && tx.bundle ? (
            <span className="font-semibold text-strong">
              {tx.bundle.burn_r !== '0' && fmt.amount(tx.bundle.burn_r)}
              {tx.bundle.burn_r !== '0' && tx.bundle.burn_a !== '0' && ` ${t('tx.invoke.and')} `}
              {tx.bundle.burn_a !== '0' && fmt.tokenAmount(tx.bundle.burn_a, tx.bundle.burn_asset, tokens)}
            </span>
          ) : (
            <span className="text-mute">{t('tx.multisig.unfunded')}</span>
          )}
        </DetailRow>
      )}
      {a?.kind === 'multisig_pay' && (
        <>
          <DetailRow label={t('tx.multisig.payouts')}>
            <PayoutList payouts={a.pays} tokens={tokens} />
          </DetailRow>
          <DetailRow label={t('tx.multisig.signedBy')}>
            <Positions list={a.signers} />
          </DetailRow>
          <DetailRow label={t('tx.noteTime')}>
            <L href={`/blocks/${a.time}`} className="link font-mono">
              #{fmt.number(a.time)}
            </L>
          </DetailRow>
        </>
      )}
      {a?.kind === 'multisig_rotate' && (
        <>
          <DetailRow label={t('tx.multisig.newSet')}>
            <span className="font-mono">{t('tx.multisig.ofSigners', { m: fmt.number(a.threshold), n: fmt.number(a.signer_count) })}</span>
          </DetailRow>
          <DetailRow label={t('tx.multisig.signedBy')}>
            <Positions list={a.signed_by} />
          </DetailRow>
        </>
      )}
      {(a?.kind === 'multisig_pay' || a?.kind === 'multisig_rotate') && (
        <DetailRow label={t('tx.multisig.nonce')}>
          <span className="font-mono">{fmt.number(a.nonce)}</span>
        </DetailRow>
      )}
      {account && (
        <DetailRow label={t('tx.multisig.now')}>
          <span className="flex flex-col gap-1">
            <span>
              {t('tx.multisig.nowValue', {
                m: fmt.number(account.threshold),
                n: fmt.number(account.signers.length),
                nonce: fmt.number(account.nonce),
              })}
            </span>
            <span className="text-xs text-mute">
              {account.vault.length === 0
                ? t('tx.multisig.emptyVault')
                : account.vault.map((v) => fmt.tokenAmount(v.amount, v.asset, tokens)).join(' · ')}
            </span>
          </span>
        </DetailRow>
      )}
      <p className="px-4 py-3 text-xs text-mute">{help}</p>
    </Panel>
  );
}

function RandPricePanel({ tx, title }: { tx: TransactionDetail; title: string }) {
  const { t, tp } = useT();
  const fmt = useFmt();
  const { data: current } = useRandPrice();
  const a = tx.staking_action?.kind === 'set_rand_price' ? tx.staking_action : null;
  return (
    <Panel title={title}>
      <DetailRow label={t('tx.price.price')}>
        {a ? (
          <span className="text-base font-semibold text-strong">
            {t('tx.price.usdPerRand', { usd: fmt.units(a.price_micros_per_rand, 6) })}
          </span>
        ) : (
          <span className="text-mute">—</span>
        )}
      </DetailRow>
      <DetailRow label={t('tx.nonce')}>
        <span className="font-mono">{tx.action_nonce === null ? '—' : fmt.number(tx.action_nonce)}</span>
      </DetailRow>
      {a && (
        <>
          <DetailRow label={t('tx.admit.voters')}>
            <span className="font-mono">{tp('tx.admit.validators', a.voters.length)}</span>
          </DetailRow>
          <DetailRow label={t('tx.admit.votedBy')}>
            <span className="flex flex-col gap-1">
              {a.voters.map((v) => (
                <Hash key={v} value={v} href={`/validators/${v}`} start={10} end={6} />
              ))}
            </span>
          </DetailRow>
        </>
      )}
      {current && (
        <DetailRow label={t('tx.price.current')}>
          <span>
            {t('tx.price.usdPerRand', { usd: fmt.units(current.price_micros_per_rand, 6) })}{' '}
            <span className="text-xs text-mute">
              · {current.fresh ? t('tx.price.fresh') : t('tx.price.stale')} · #{fmt.number(current.set_at_height)}
            </span>
          </span>
        </DetailRow>
      )}
      <p className="px-4 py-3 text-xs text-mute">{t('tx.price.help')}</p>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// Receipt
// ---------------------------------------------------------------------------

function ReceiptPanel({ receipt }: { receipt: Receipt | null }) {
  const { t } = useT();
  const fmt = useFmt();
  if (!receipt) {
    return (
      <Panel title={t('tx.receipt.title')}>
        <div className="py-6 text-sm text-mute">{t('tx.receipt.none')}</div>
      </Panel>
    );
  }

  return (
    <Panel title={t('tx.receipt.title')}>
      <DetailRow label={t('tx.program')}>
        <Hash value={receipt.program} href={`/programs/${receipt.program}`} full />
      </DetailRow>
      <DetailRow label={t('tx.receipt.tier')}>
        <span className="font-mono">{fmt.number(receipt.tier)}</span>
      </DetailRow>
      <DetailRow label={t('tx.receipt.position')}>
        <span className="inline-flex flex-wrap items-center gap-2">
          <L href={`/blocks/${receipt.height}`} className="link font-mono">
            #{fmt.number(receipt.height)}
          </L>
          <span className="text-mute">· {t('tx.receipt.index', { index: fmt.number(receipt.index) })}</span>
        </span>
      </DetailRow>
      <DetailRow label={t('tx.receipt.hIn')}>
        {receipt.h_in ? <Hash value={receipt.h_in} full /> : <span className="text-mute">—</span>}
      </DetailRow>
      <DetailRow label={t('tx.receipt.hPub')}>
        {receipt.h_pub ? (
          <Hash value={receipt.h_pub} full />
        ) : (
          <span className="text-mute">{t('tx.receipt.hPubEmpty')}</span>
        )}
      </DetailRow>
      <DetailRow label={t('tx.receipt.outputs')}>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          {receipt.outputs.map((output, index) => (
            <div key={index} className="rounded border border-border bg-bg-soft px-3 py-2">
              <div className="text-xs font-semibold text-soft">
                {t('tx.receipt.out', { index })}
              </div>
              <div className="truncate font-mono text-sm text-text" title={String(output)}>
                {fmt.number(output)}
              </div>
            </div>
          ))}
        </div>
      </DetailRow>
      <p className="px-4 py-3 text-xs text-mute">{t('tx.receipt.help')}</p>
    </Panel>
  );
}
