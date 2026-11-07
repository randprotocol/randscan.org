'use client';

import { Mark } from './Brand';
import { L, useT } from '@/i18n/client';

/** Explorer sections repeated in the footer, so every page links to them at the bottom too. */
const explorerLinks = [
  { href: '/blocks', key: 'blocks' },
  { href: '/transactions', key: 'transactions' },
  { href: '/validators', key: 'validators' },
  { href: '/programs', key: 'programs' },
  { href: '/tokens', key: 'tokens' },
  { href: '/bridge', key: 'bridge' },
];

const linkClass = 'text-invert-ink-3 transition-colors hover:text-invert-ink';

/** The site's one inverted band: blurb, two columns of small links, a mono legal line. */
export function Footer() {
  const { t } = useT();
  return (
    <footer className="bg-invert text-invert-ink">
      <div className="mx-auto max-w-[1200px] px-4 pb-8 pt-14 sm:px-6">
        <div className="grid gap-10 sm:grid-cols-2 lg:grid-cols-[2fr_1fr_1fr]">
          <p className="brand max-w-xs items-start gap-3 text-sm text-invert-ink">
            <Mark size={16} className="mt-0.5 !text-invert-ink [&_.hot]:!fill-invert-ink-3" />
            <span>{t('footer.blurb')}</span>
          </p>
          <ul className="space-y-2.5 text-[13px]">
            {explorerLinks.map((link) => (
              <li key={link.href}>
                <L href={link.href} className={linkClass}>
                  {t(`header.nav.${link.key}`)}
                </L>
              </li>
            ))}
          </ul>
          <ul className="space-y-2.5 text-[13px]">
            <li>
              <a href="https://randprotocol.org" target="_blank" rel="noopener noreferrer" className={linkClass}>
                randprotocol.org
              </a>
            </li>
            <li>
              <a href="https://github.com/randprotocol" target="_blank" rel="noopener noreferrer" className={linkClass}>
                GitHub
              </a>
            </li>
            <li>
              <L href="/privacy" className={linkClass}>
                {t('footer.privacy')}
              </L>
            </li>
            <li>
              <L href="/terms" className={linkClass}>
                {t('footer.terms')}
              </L>
            </li>
          </ul>
        </div>
        <p className="mt-14 border-t border-invert-rule pt-5 font-mono text-[11px] uppercase tracking-[0.08em] text-invert-ink-3">
          {'RandScan · randscan.org'}
        </p>
      </div>
    </footer>
  );
}
