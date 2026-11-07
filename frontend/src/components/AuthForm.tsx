'use client';

import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { useSWRConfig } from 'swr';
import * as api from '@/lib/api';
import { L, useLocalizePath, useT } from '@/i18n/client';

interface AuthFormProps {
  mode: 'login' | 'signup';
}

const altLinks = { login: '/signup', signup: '/login' } as const;

/** Email + password form used by /login and /signup. Redirects to /dashboard on success. */
export function AuthForm({ mode }: AuthFormProps) {
  const router = useRouter();
  const { t, rich } = useT();
  const l = useLocalizePath();
  const { mutate } = useSWRConfig();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const k = `auth.form.${mode}`;

  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const user = mode === 'login' ? await api.login(email, password) : await api.signup(email, password);
      await mutate('me', user, { revalidate: false });
      router.push(l('/dashboard'));
    } catch (err) {
      setError(err instanceof api.ApiError ? err.message : t('auth.form.requestFailed'));
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto max-w-md">
      <h1 className="text-3xl font-medium text-strong">{t(`${k}.title`)}</h1>
      <p className="mt-2 text-sm text-soft">
        {t('auth.form.intro')}
      </p>
      <form onSubmit={onSubmit} className="card-padded mt-6 space-y-4">
        <div>
          <label htmlFor="email" className="field-label">
            {t('auth.form.email')}
          </label>
          <input
            id="email"
            type="email"
            autoComplete="email"
            required
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="input"
          />
        </div>
        <div>
          <label htmlFor="password" className="field-label">
            {t('auth.form.password')}
          </label>
          <input
            id="password"
            type="password"
            autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
            required
            minLength={10}
            maxLength={128}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="input"
          />
          {mode === 'signup' && <p className="mt-1.5 text-xs text-mute">{t('auth.form.minLength')}</p>}
          {mode === 'login' && (
            <p className="mt-1.5 text-end text-xs">
              <L href="/forgot" className="link">
                {t('auth.form.forgotLink')}
              </L>
            </p>
          )}
        </div>
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        <button type="submit" disabled={busy} className="btn-primary w-full justify-center disabled:opacity-60">
          {busy ? t('auth.form.pleaseWait') : t(`${k}.button`)}
        </button>
        {mode === 'signup' && (
          <p className="text-center text-xs text-mute">{rich('auth.form.signup.terms')}</p>
        )}
        <p className="text-center text-sm text-soft">
          {t(`${k}.alt`)}{' '}
          <L href={altLinks[mode]} className="link">
            {t(`${k}.altLabel`)}
          </L>
        </p>
      </form>
    </div>
  );
}
