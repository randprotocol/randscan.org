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

  // On the home page the hero carries the search, so the header does not repeat it.
  const onHome = pathname === '/';

  return (
    <header className="border-b border-border bg-bg">
      <div className="mx-auto max-w-[1200px] px-4 sm:px-6">
        <div className="flex h-16 items-center justify-between gap-5">
          <L href="/" className="flex flex-shrink-0 items-center">
            <Brand />
          </L>

          <div className="ms-auto hidden items-center gap-2 lg:flex">
            {!onHome && (
              <div className="w-72">
                <SearchBar />
              </div>
            )}
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
            <AccountLink signedIn={!!me} className="btn-primary ms-2 min-h-9 px-3.5 text-[13px]" />
          </div>

          <div className="ms-auto flex items-center gap-1 lg:hidden">
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
                  strokeLinecap="square"
                  strokeLinejoin="miter"
                  d={mobileOpen ? 'M6 18L18 6M6 6l12 12' : 'M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5'}
                />
              </svg>
            </button>
          </div>
        </div>

        <nav className="-mx-1 hidden h-11 items-stretch gap-6 overflow-x-auto border-t border-border-soft px-1 lg:flex">
          {navLinks.map((link) => {
            const active = isActive(link.href);
            return (
              <L
                key={link.href}
                href={link.href}
                aria-current={active ? 'page' : undefined}
                className={cn(
                  'relative flex items-center whitespace-nowrap text-sm transition-colors hover:text-strong',
                  active ? 'text-strong' : 'text-text'
                )}
              >
                {t(`header.nav.${link.key}`)}
                {active && <span className="absolute -bottom-px start-0 h-px w-full bg-ink" />}
              </L>
            );
          })}
        </nav>

        {mobileOpen && (
          <nav className="flex flex-col border-t border-border-soft py-2 lg:hidden">
            {navLinks.map((link) => (
              <L
                key={link.href}
                href={link.href}
                onClick={() => setMobileOpen(false)}
                aria-current={isActive(link.href) ? 'page' : undefined}
                className={cn(
                  'flex items-center gap-2 border-b border-border-soft px-1 py-2.5 text-sm transition-colors last:border-0 hover:text-strong',
                  isActive(link.href) ? 'text-strong' : 'text-text'
                )}
              >
                {t(`header.nav.${link.key}`)}
              </L>
            ))}
            <AccountLink
              signedIn={!!me}
              className="btn-primary mt-3 w-full"
              onClick={() => setMobileOpen(false)}
            />
            <LanguageMenu inline className="mt-3 border-t border-border-soft pt-2" />
          </nav>
        )}

        {!onHome && (
          <div className="pb-4 lg:hidden">
            <SearchBar />
          </div>
        )}
      </div>
    </header>
  );
}
