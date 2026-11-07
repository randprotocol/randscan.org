'use client';

import { useRouter, useSearchParams } from 'next/navigation';
import { useState } from 'react';
import { useSWRConfig } from 'swr';
import * as api from '@/lib/api';
import { L, useLocalizePath, useT } from '@/i18n/client';

/** The API's own message as received; the fallback is ours. */
function errorMessage(err: unknown, fallback: string): string {
  return err instanceof api.ApiError ? err.message : fallback;
}

/** /forgot: asks for an email and always reports success (no account enumeration). */
export function ForgotPasswordForm() {
  const { t, rich } = useT();
  const [email, setEmail] = useState('');
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await api.forgotPassword(email);
      setSent(true);
    } catch (err) {
      setError(errorMessage(err, t('auth.form.requestFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto max-w-md">
      <h1 className="text-3xl font-medium text-strong">{t('auth.forgot.title')}</h1>
      <p className="mt-2 text-sm text-soft">
        {t('auth.forgot.intro')}
      </p>
      {sent ? (
        <div className="card-padded mt-6 space-y-3">
          <p className="text-sm text-text">{rich('auth.forgot.sent', { email: email.trim() })}</p>
          <L href="/login" className="link text-sm">
            {t('auth.forgot.backToSignIn')}
          </L>
        </div>
      ) : (
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
          {error && (
            <div className="form-error" role="alert">
              {error}
            </div>
          )}
          <button type="submit" disabled={busy} className="btn-primary w-full justify-center disabled:opacity-60">
            {busy ? t('auth.form.pleaseWait') : t('auth.forgot.button')}
          </button>
          <p className="text-center text-sm text-soft">
            {t('auth.forgot.remembered')}{' '}
            <L href="/login" className="link">
              {t('auth.forgot.signIn')}
            </L>
          </p>
        </form>
      )}
    </div>
  );
}

/** /reset?token=…: sets a new password with the emailed token, then signs the user in. */
export function ResetPasswordForm() {
  const router = useRouter();
  const { t } = useT();
  const l = useLocalizePath();
  const { mutate } = useSWRConfig();
  const token = (useSearchParams().get('token') ?? '').trim();
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    if (password !== confirm) {
      setError(t('auth.reset.mismatch'));
      return;
    }
    setBusy(true);
    try {
      const user = await api.resetPassword(token, password);
      await mutate('me', user, { revalidate: false });
      router.push(l('/dashboard'));
    } catch (err) {
      setError(errorMessage(err, t('auth.form.requestFailed')));
      setBusy(false);
    }
  };

  if (!token) {
    return (
      <div className="mx-auto max-w-md">
        <h1 className="text-3xl font-medium text-strong">{t('auth.reset.title')}</h1>
        <div className="card-padded mt-6 space-y-3">
          <p className="text-sm text-text">{t('auth.reset.missingToken')}</p>
          <L href="/forgot" className="link text-sm">
            {t('auth.reset.requestLink')}
          </L>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-md">
      <h1 className="text-3xl font-medium text-strong">{t('auth.reset.choose')}</h1>
      <p className="mt-2 text-sm text-soft">{t('auth.reset.signedOut')}</p>
      <form onSubmit={onSubmit} className="card-padded mt-6 space-y-4">
        <div>
          <label htmlFor="password" className="field-label">
            {t('auth.reset.newPassword')}
          </label>
          <input
            id="password"
            type="password"
            autoComplete="new-password"
            required
            minLength={10}
            maxLength={128}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="input"
          />
          <p className="mt-1.5 text-xs text-mute">{t('auth.form.minLength')}</p>
        </div>
        <div>
          <label htmlFor="confirm" className="field-label">
            {t('auth.reset.confirm')}
          </label>
          <input
            id="confirm"
            type="password"
            autoComplete="new-password"
            required
            minLength={10}
            maxLength={128}
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            className="input"
          />
        </div>
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        <button type="submit" disabled={busy} className="btn-primary w-full justify-center disabled:opacity-60">
          {busy ? t('auth.form.pleaseWait') : t('auth.reset.button')}
        </button>
        <p className="text-center text-sm text-soft">
          {t('auth.reset.expired')}{' '}
          <L href="/forgot" className="link">
            {t('auth.reset.requestNew')}
          </L>
        </p>
      </form>
    </div>
  );
}

/** Dashboard panel body: change the password of the signed-in user. */
export function ChangePasswordForm() {
  const { t } = useT();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    setDone(false);
    setBusy(true);
    try {
      await api.changePassword(current, next);
      setCurrent('');
      setNext('');
      setDone(true);
    } catch (err) {
      setError(errorMessage(err, t('auth.form.requestFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={onSubmit} className="space-y-4 py-3">
      <div className="grid gap-4 sm:grid-cols-2">
        <div>
          <label htmlFor="current-password" className="field-label">
            {t('auth.change.current')}
          </label>
          <input
            id="current-password"
            type="password"
            autoComplete="current-password"
            required
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
            className="input"
          />
        </div>
        <div>
          <label htmlFor="new-password" className="field-label">
            {t('auth.reset.newPassword')}
          </label>
          <input
            id="new-password"
            type="password"
            autoComplete="new-password"
            required
            minLength={10}
            maxLength={128}
            value={next}
            onChange={(e) => setNext(e.target.value)}
            className="input"
          />
        </div>
      </div>
      {error && (
        <div className="form-error" role="alert">
          {error}
        </div>
      )}
      {done && <p className="text-sm text-positive">{t('auth.change.done')}</p>}
      <button type="submit" disabled={busy} className="btn-secondary disabled:opacity-60">
        {busy ? t('auth.form.pleaseWait') : t('auth.change.button')}
      </button>
    </form>
  );
}
