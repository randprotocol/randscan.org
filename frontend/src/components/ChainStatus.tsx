'use client';

// The site-sync pill (in the header) and banner (under it) on every page, from GET /api/v1/chain
// every 15 s, classified here against the build's own pin (src/lib/chainPin.ts) with the shared
// rules (src/lib/chainStatus.ts). The data on the page stays visible whatever the state; the
// banner only says it may be stale, with the last good height and its age.

import { useChain } from '@/hooks/useApi';
import { EXPECTED_CHAIN } from '@/lib/chainPin';
import { bannerMessage, classify, tone, type ChainView, type Classification } from '@/lib/chainStatus';
import { cn } from '@/lib/utils';
import { useFmt, useT } from '@/i18n/client';

const TONE_TEXT = {
  positive: 'text-positive',
  warning: 'text-warning',
  negative: 'text-negative',
  mute: 'text-mute',
} as const;

/** The page's view and its verdict; `null` until the first answer (or failure) arrives. */
function useChainStatus(): { view: ChainView | undefined; status: Classification | null } {
  const { data, error, isLoading } = useChain();
  if (isLoading && !data && !error) return { view: undefined, status: null };
  // A failed poll classifies as unreachable even while SWR still holds the last good view.
  const status = classify(error ? null : (data ?? null), EXPECTED_CHAIN, Date.now());
  return { view: data, status };
}

export function ChainPill({ className }: { className?: string }) {
  const { t, locale } = useT();
  const fmt = useFmt();
  const { view, status } = useChainStatus();
  const state = status?.state ?? null;
  const label = t(`chainStatus.states.${state ?? 'checking'}`);
  return (
    <a
      href="/api/v1/chain"
      target="_blank"
      rel="noopener noreferrer"
      data-testid="chain-pill"
      data-state={state ?? 'checking'}
      title={status ? `${t('chainStatus.pillTitle', { state: label })} ${status.detail}` : label}
      className={cn(
        'inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border border-border-soft px-2.5 py-1 text-2xs text-soft transition-colors hover:text-strong',
        className,
      )}
      lang={locale}
    >
      <span aria-hidden="true" className={TONE_TEXT[tone(state)]}>
        ●
      </span>
      <span>{t('chainStatus.pill', { chain: EXPECTED_CHAIN.chain_id })}</span>
      <span aria-hidden="true">·</span>
      <span className={TONE_TEXT[tone(state)]}>{label}</span>
      {view && view.head_height > 0 && (
        <>
          <span aria-hidden="true">·</span>
          <span className="font-mono">#{fmt.number(view.head_height)}</span>
        </>
      )}
    </a>
  );
}

export function ChainBanner() {
  const { t } = useT();
  const fmt = useFmt();
  const { view, status } = useChainStatus();
  const msg = status ? bannerMessage(status, view, EXPECTED_CHAIN) : null;
  if (!status || !msg) return null;
  const age = msg.lastGood ? fmt.ago(msg.lastGood.timestampMs) : '—';
  return (
    <div
      role="alert"
      data-testid="chain-banner"
      data-state={status.state}
      className={cn(
        'border-b px-6 py-2.5 text-sm text-strong lg:px-8',
        status.state === 'behind' ? 'chain-banner-behind' : 'chain-banner',
      )}
    >
      <div className="mx-auto max-w-6xl">
        <span className="font-medium">{t(msg.key, { ...msg.vars, age })}</span>
        {msg.lastGood && (
          <span className="ms-2 text-soft">
            {t('chainStatus.lastGood', { height: fmt.number(msg.lastGood.height), age })}
          </span>
        )}
      </div>
    </div>
  );
}
