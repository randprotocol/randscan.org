'use client';

import { usePathname, useRouter } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import { LOCALES, localizePath, stripLocale } from '@/i18n/locales';
import { useLocale, useT } from '@/i18n/client';
import { cn } from '@/lib/utils';

/**
 * The twenty languages. Choosing one remembers it in the `lang` cookie (which wins over the
 * visitor's country from then on, src/middleware.ts) and opens the same page in that language.
 * `inline` renders the plain list for the mobile drawer; otherwise a globe icon opens it.
 */
export function LanguageMenu({ className, inline = false }: { className?: string; inline?: boolean }) {
  const { code } = useLocale();
  const { t } = useT();
  const pathname = usePathname();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', close);
    document.addEventListener('keydown', esc);
    return () => {
      document.removeEventListener('mousedown', close);
      document.removeEventListener('keydown', esc);
    };
  }, [open]);

  const choose = (next: string) => {
    document.cookie = `lang=${next}; Path=/; Max-Age=31536000; SameSite=Lax`;
    setOpen(false);
    const bare = stripLocale(pathname ?? '/') + window.location.search + window.location.hash;
    router.push(localizePath(next, bare));
  };

  const list = (
    <ul
      aria-label={t('common.language')}
      className={cn(inline ? 'grid grid-cols-2 gap-x-4' : 'max-h-80 overflow-y-auto py-1')}
    >
      {LOCALES.map((l) => (
        <li key={l.code}>
          <button
            type="button"
            lang={l.tag}
            dir={l.dir}
            aria-current={l.code === code ? 'true' : undefined}
            onClick={() => choose(l.code)}
            className={cn(
              'block w-full py-1.5 text-start text-[13px] transition-colors hover:text-strong',
              inline ? 'px-1' : 'px-3',
              l.code === code ? 'text-strong' : 'text-soft'
            )}
          >
            {l.name}
          </button>
        </li>
      ))}
    </ul>
  );

  if (inline) return <div className={className}>{list}</div>;

  return (
    <div ref={ref} className={cn('relative', className)}>
      <button
        type="button"
        className="btn-icon"
        aria-haspopup="true"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        title={t('common.language')}
      >
        <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.6} aria-hidden="true">
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3z" />
        </svg>
        <span className="sr-only">{t('common.language')}</span>
      </button>
      {open && (
        <div className="absolute end-0 top-full z-50 mt-1 w-48 rounded-sm border border-border bg-surface">
          {list}
        </div>
      )}
    </div>
  );
}
