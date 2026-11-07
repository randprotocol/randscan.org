'use client';

import { useEffect, useState } from 'react';
import { useT } from '@/i18n/client';

type Theme = 'light' | 'dark';

const STORAGE_KEY = 'randscan-theme';

function currentTheme(): Theme {
  if (typeof document === 'undefined') return 'light';
  return document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
}

function storedTheme(): Theme | null {
  try {
    const t = localStorage.getItem(STORAGE_KEY);
    return t === 'dark' || t === 'light' ? t : null;
  } catch {
    return null;
  }
}

/**
 * Flips `data-theme` on <html>. The initial value is applied by the inline script in layout.tsx
 * (stored choice, else the operating system's), so this mirrors it, persists a choice, and keeps
 * following the system until the user makes one.
 */
export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>('light');
  const [mounted, setMounted] = useState(false);
  const { t } = useT();

  useEffect(() => {
    setTheme(currentTheme());
    setMounted(true);
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const follow = () => {
      if (storedTheme()) return;
      const next: Theme = mq.matches ? 'dark' : 'light';
      document.documentElement.setAttribute('data-theme', next);
      setTheme(next);
    };
    mq.addEventListener('change', follow);
    return () => mq.removeEventListener('change', follow);
  }, []);

  const toggle = () => {
    const next: Theme = currentTheme() === 'dark' ? 'light' : 'dark';
    document.documentElement.setAttribute('data-theme', next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Storage unavailable (private mode); the choice just won't persist.
    }
    setTheme(next);
  };

  return (
    <button
      type="button"
      onClick={toggle}
      className="btn-icon"
      aria-label={theme === 'dark' ? t('components.theme.toLight') : t('components.theme.toDark')}
      title={theme === 'dark' ? t('components.theme.light') : t('components.theme.dark')}
    >
      {/* Before mount the stored theme is unknown, so render the moon as a
          neutral placeholder to keep server and client markup identical. */}
      {mounted && theme === 'light' ? (
        <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.6}>
          <circle cx="12" cy="12" r="4" />
          <path
            strokeLinecap="round"
            d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"
          />
        </svg>
      ) : (
        <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.6}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"
          />
        </svg>
      )}
    </button>
  );
}
