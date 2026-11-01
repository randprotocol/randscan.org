'use client';

import { usePathname } from 'next/navigation';
import { useState } from 'react';
import { SearchBar } from './SearchBar';
import { ThemeToggle } from './ThemeToggle';
import { Brand } from './Brand';
import { cn } from '@/lib/utils';
import { useMe } from '@/hooks/useApi';
import { L, useT } from '@/i18n/client';
import { stripLocale } from '@/i18n/locales';
import { LanguageMenu } from './LanguageMenu';

const navLinks = [
  { href: '/', key: 'dashboard' },
  { href: '/blocks', key: 'blocks' },
  { href: '/transactions', key: 'transactions' },
  { href: '/notes', key: 'notes' },
  { href: '/viewing', key: 'history' },
  { href: '/validators', key: 'validators' },
  { href: '/provers', key: 'provers' },
  { href: '/programs', key: 'programs' },
  { href: '/tokens', key: 'tokens' },
  { href: '/bridge', key: 'bridge' },
  { href: '/nodes', key: 'nodes' },
];

function AccountLink({
  signedIn,
  className,
  onClick,
}: {
  signedIn: boolean;
  className?: string;
  onClick?: () => void;
}) {
  const { t } = useT();
  return (
    <L href={signedIn ? '/dashboard' : '/login'} className={className} onClick={onClick}>
      {signedIn ? t('header.apiKeys') : t('header.signIn')}
    </L>
  );
}

export function Header() {
  const pathname = stripLocale(usePathname() ?? '/');
  const { t } = useT();
  const [mobileOpen, setMobileOpen] = useState(false);
  const { data: me } = useMe();

  const isActive = (href: string) =>
    href === '/' ? pathname === '/' : pathname.startsWith(href);

  return (
    <header className="border-b border-border-soft bg-bg">
      <div className="mx-auto max-w-6xl px-6 lg:px-8">
        <div className="flex h-14 items-center justify-between gap-5">
          <L href="/" className="flex flex-shrink-0 items-center rounded-md">
            <Brand />
          </L>

          <nav className="hidden items-center gap-4 lg:flex">
            {navLinks.map((link) => {
              const active = isActive(link.href);
              return (
                <L
                  key={link.href}
                  href={link.href}
                  className={cn(
                    'relative whitespace-nowrap py-1 text-[13px] transition-colors hover:text-strong',
                    active ? 'text-strong' : 'text-soft'
                  )}
                >
                  {t(`header.nav.${link.key}`)}
                  {active && (
                    <span className="absolute -bottom-px start-0 h-0.5 w-full rounded-full bg-accent" />
                  )}
                </L>
              );
            })}
          </nav>

          <div className="ms-auto hidden items-center gap-2.5 lg:flex">
            <div className="w-44">
              <SearchBar />
            </div>
            <AccountLink signedIn={!!me} className="whitespace-nowrap text-[13px] text-soft transition-colors hover:text-strong" />
            <a
              href="https://github.com/randprotocol"
              target="_blank"
              rel="noopener noreferrer"
              className="btn-icon"
              title="GitHub"
            >
              <svg className="h-4 w-4" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
              </svg>
              <span className="sr-only">GitHub</span>
            </a>
            <LanguageMenu />
            <ThemeToggle />
          </div>

          <div className="ms-auto flex items-center gap-2 lg:hidden">
            <ThemeToggle />
            <button
              type="button"
              onClick={() => setMobileOpen((open) => !open)}
              aria-expanded={mobileOpen}
              className="btn-icon"
            >
              <span className="sr-only">{t('header.toggleNav')}</span>
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={1.6} stroke="currentColor">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d={mobileOpen ? 'M6 18L18 6M6 6l12 12' : 'M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5'}
                />
              </svg>
            </button>
          </div>
        </div>

        {mobileOpen && (
          <nav className="flex flex-col border-t border-border-soft py-2 lg:hidden">
            {navLinks.map((link) => (
              <L
                key={link.href}
                href={link.href}
                onClick={() => setMobileOpen(false)}
                className={cn(
                  'px-1 py-2 text-sm transition-colors hover:text-strong',
                  isActive(link.href) ? 'text-strong' : 'text-soft'
                )}
              >
                {t(`header.nav.${link.key}`)}
              </L>
            ))}
            <AccountLink
              signedIn={!!me}
              className="px-1 py-2 text-sm text-soft transition-colors hover:text-strong"
              onClick={() => setMobileOpen(false)}
            />
            <LanguageMenu inline className="mt-2 border-t border-border-soft pt-2" />
          </nav>
        )}

        <div className="pb-4 lg:hidden">
          <SearchBar />
        </div>
      </div>
    </header>
  );
}
