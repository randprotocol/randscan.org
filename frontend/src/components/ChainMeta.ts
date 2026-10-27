// The chain pin as <meta> tags in every page's <head> (rand-genesis, rand-chain-id). Written with
// createElement rather than JSX so tests/chain-status.test.mjs can render it under plain Node.
// A server component: the tags are in the static HTML, where the site-sync cron looks for them;
// a pin only in a lazily loaded JS chunk could go unseen.
import { createElement, Fragment } from 'react';
import { chainMetaTags, EXPECTED_CHAIN, type ChainPin } from '@/lib/chainPin';

export function ChainMeta({ pin = EXPECTED_CHAIN }: { pin?: ChainPin }) {
  return createElement(
    Fragment,
    null,
    ...chainMetaTags(pin).map((m) => createElement('meta', { key: m.name, name: m.name, content: m.content })),
  );
}
