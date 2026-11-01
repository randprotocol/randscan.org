'use client';

// The site-sync pill in the header on every page, from GET /api/v1/chain every 15 s, classified
// here against the build's own pin (src/lib/chainPin.ts) with the shared rules
// (src/lib/chainStatus.ts). The data on the page stays visible whatever the state.

import { useChain } from '@/hooks/useApi';
import { EXPECTED_CHAIN } from '@/lib/chainPin';
import { classify, tone, type ChainView, type Classification } from '@/lib/chainStatus';
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
