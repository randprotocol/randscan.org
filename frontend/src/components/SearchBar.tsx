'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useLocalizePath, useT } from '@/i18n/client';
import { cn } from '@/lib/utils';

interface SearchBarProps {
  className?: string;
  placeholder?: string;
  defaultValue?: string;
  autoFocus?: boolean;
  /** `lg`: the home page hero's search, taller, with a labelled submit button. */
  size?: 'md' | 'lg';
}

/** Navigates to `/search?q=…`; the API decides what the query refers to. */
export function SearchBar({
  className,
  placeholder,
  defaultValue = '',
  autoFocus = false,
  size = 'md',
}: SearchBarProps) {
  const router = useRouter();
  const { t } = useT();
  const l = useLocalizePath();
  const [query, setQuery] = useState(defaultValue);

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    const trimmed = query.trim();
    if (!trimmed) return;
    router.push(l(`/search?q=${encodeURIComponent(trimmed)}`));
  };

  const lg = size === 'lg';

  return (
    <form onSubmit={handleSubmit} className={cn('relative', lg && 'flex items-stretch', className)}>
      <svg
        className={cn(
          'pointer-events-none absolute top-1/2 -translate-y-1/2 text-mute',
          lg ? 'start-4 h-[18px] w-[18px]' : 'start-2.5 h-4 w-4'
        )}
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={lg ? 1.6 : 2}
      >
        <path strokeLinecap="square" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <input
        type="text"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder={placeholder ?? t('components.search.placeholder')}
        // eslint-disable-next-line jsx-a11y/no-autofocus
        autoFocus={autoFocus}
        spellCheck={false}
        aria-label={t('components.search.ariaLabel')}
        className={cn(
          'input',
          lg ? 'h-14 min-w-0 flex-1 rounded-e-none border-e-0 ps-11 pe-3 text-[15px]' : 'py-1.5 ps-8 pe-3'
        )}
      />
      {lg && (
        <button type="submit" className="btn-primary h-14 flex-none rounded-s-none px-6 text-[15px]">
          {t('search.title')}
        </button>
      )}
    </form>
  );
}
