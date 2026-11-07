'use client';

import { Column, DataTable } from '@/components/DataTable';
import { Hash } from '@/components/Hash';
import { ErrorState, PageHeader } from '@/components/States';
import { useAdmittedValidators, useValidators } from '@/hooks/useApi';
import { L, useFmt, useT, type Fmt } from '@/i18n/client';
import type { Validator } from '@/types';

function pendingTotal(v: Validator): bigint {
  return v.pending.reduce((sum, p) => {
    try {
      return sum + BigInt(p.amount || '0');
    } catch {
      return sum;
    }
  }, BigInt(0));
}

const validatorColumns = (t: (key: string) => string, fmt: Fmt): Column<Validator>[] => [
  {
    key: 'address',
    header: t('validators.columns.validator'),
    render: (validator) => (
      <Hash value={validator.address} href={`/validators/${validator.address}`} start={10} end={6} />
    ),
  },
  {
    key: 'active',
    header: t('validators.columns.epochSet'),
    render: (validator) =>
      validator.active ? (
        <span className="badge badge-accent">{t('validators.active')}</span>
      ) : (
        <span className="badge badge-neutral">{t('validators.inactive')}</span>
      ),
  },
  {
    key: 'stake',
    header: t('validators.columns.stake'),
    render: (validator) => <span className="text-text">{fmt.stake(validator.stake)}</span>,
  },
  {
    key: 'rewards',
    header: t('validators.columns.rewards'),
    render: (validator) => <span className="text-soft">{fmt.stake(validator.rewards)}</span>,
  },
  {
    key: 'pending',
    header: t('validators.columns.unbonding'),
    render: (validator) =>
      validator.pending.length === 0 ? (
        <span className="text-mute">—</span>
      ) : (
        <span className="text-soft">
          {fmt.stake(pendingTotal(validator))}
          <span className="ms-1 text-mute">({validator.pending.length})</span>
        </span>
      ),
  },
  {
    key: 'share_percent',
    header: t('validators.columns.share'),
    render: (validator) => (
      <div className="flex items-center gap-2">
        <div className="h-1 w-20 overflow-hidden bg-rule-soft">
          <div
            className="h-full bg-ink"
            style={{ width: `${Math.min(100, Math.max(0, validator.share_percent))}%` }}
          />
        </div>
        <span className="text-soft">{fmt.percentage(validator.share_percent)}</span>
      </div>
    ),
  },
  {
    key: 'blocks_proposed',
    header: t('validators.columns.blocksProposed'),
    render: (validator) => (
      <span className="text-soft">{fmt.number(validator.blocks_proposed)}</span>
    ),
  },
  {
    key: 'last_proposed',
    header: t('validators.columns.lastProposed'),
    render: (validator) =>
      validator.last_proposed_height === null ? (
        <span className="text-mute">{t('validators.never')}</span>
      ) : (
        <span className="text-soft">
          <L href={`/blocks/${validator.last_proposed_height}`} className="link font-mono">
            #{fmt.number(validator.last_proposed_height)}
          </L>
          {validator.last_proposed_timestamp_ms !== null && (
            <span className="ms-2 text-mute">
              {fmt.ago(validator.last_proposed_timestamp_ms)}
            </span>
          )}
        </span>
      ),
  },
];

export default function ValidatorsPage() {
  const { data, error, isLoading, mutate } = useValidators();
  const { data: admitted } = useAdmittedValidators();
  const { t, tp, rich } = useT();
  const fmt = useFmt();
  const columns = validatorColumns(t, fmt);

  if (error && !data) {
    return (
      <>
        <PageHeader title={t('validators.title')} />
        <ErrorState message={t('validators.error')} onRetry={() => void mutate()} />
      </>
    );
  }

  const active = (data ?? []).filter((v) => v.active);
  const activeStake = active.reduce((sum, v) => {
    try {
      return sum + BigInt(v.stake || '0');
    } catch {
      return sum;
    }
  }, BigInt(0));

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('validators.title')}
        subtitle={
          data
            ? t('validators.subtitle', {
                active: fmt.number(active.length),
                total: fmt.number(data.length),
                stake: fmt.stake(activeStake, t('validators.activeStake')),
              })
            : t('validators.loading')
        }
      />

      <DataTable
        columns={columns}
        data={data ?? []}
        keyExtractor={(validator) => validator.address}
        isLoading={isLoading && !data}
        emptyMessage={t('validators.empty')}
      />

      {admitted?.admission_by_vote && (
        <div className="text-xs text-mute">
          <p>
            {rich('validators.admission')}{' '}
            {admitted.admitted.length === 0
              ? t('validators.noneAdmitted')
              : tp('validators.admittedWaiting', admitted.admitted.length)}
          </p>
          {admitted.admitted.length > 0 && (
            <ul className="mt-1 space-y-0.5">
              {admitted.admitted.map((a) => (
                <li key={a}>
                  <Hash value={a} start={10} end={6} />
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <p className="text-xs text-mute">
        {t('validators.help')}
      </p>
    </div>
  );
}
