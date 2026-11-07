'use client';

import { L, useT } from '@/i18n/client';
import { cn } from '@/lib/utils';

/**
 * Section eyebrow: mono, uppercase, a 6px square bullet, or a two-digit index in its place
 * ("01 — Latest blocks").
 */
export function SectionLabel({
  children,
  index,
  className,
}: {
  children: React.ReactNode;
  index?: string;
  className?: string;
}) {
  if (index) {
    return (
      <span className={cn('eyebrow', className)}>
        <span className="text-strong">{index}</span>
        <span aria-hidden="true">—</span>
        {children}
      </span>
    );
  }
  return <span className={cn('chip', className)}>{children}</span>;
}

interface SectionHeadingProps {
  label: string;
  index?: string;
  title?: string;
  actions?: React.ReactNode;
  className?: string;
}

export function SectionHeading({ label, index, title, actions, className }: SectionHeadingProps) {
  return (
    <div className={cn('flex flex-wrap items-end justify-between gap-3', className)}>
      <div>
        <SectionLabel index={index}>{label}</SectionLabel>
        {title && (
          <h2 className="mt-2 text-2xl font-medium text-strong">
            {title}
          </h2>
        )}
      </div>
      {actions}
    </div>
  );
}

interface PageHeaderProps {
  title: string;
  label?: string;
  subtitle?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}

export function PageHeader({ title, label, subtitle, actions, className }: PageHeaderProps) {
  return (
    <div className={cn('mb-8 flex flex-wrap items-start justify-between gap-4', className)}>
      <div className="min-w-0">
        {label && <SectionLabel className="mb-3">{label}</SectionLabel>}
        <h1 className="text-[32px] font-medium text-strong sm:text-[40px]">{title}</h1>
        {subtitle && <div className="mt-2 text-sm text-soft">{subtitle}</div>}
      </div>
      {actions && <div className="flex items-center gap-3">{actions}</div>}
    </div>
  );
}

interface NotFoundStateProps {
  title?: string;
  message: string;
  backHref?: string;
  backLabel?: string;
}

export function NotFoundState({
  title,
  message,
  backHref = '/',
  backLabel,
}: NotFoundStateProps) {
  const { t } = useT();
  return (
    <div className="card-padded py-16 text-center">
      <h2 className="text-2xl font-medium text-strong">{title ?? t('common.notFound')}</h2>
      <p className="mx-auto mt-3 max-w-md break-all text-sm text-soft">{message}</p>
      <L href={backHref} className="btn-secondary mt-7">
        {backLabel ?? t('common.backToDashboard')}
      </L>
    </div>
  );
}

interface ErrorStateProps {
  title?: string;
  message?: string;
  onRetry?: () => void;
}

export function ErrorState({
  title,
  message,
  onRetry,
}: ErrorStateProps) {
  const { t } = useT();
  return (
    <div className="card-padded py-16 text-center">
      <h2 className="text-2xl font-medium text-negative">{title ?? t('common.somethingWrong')}</h2>
      <p className="mx-auto mt-3 max-w-md text-sm text-soft">{message ?? t('common.apiUnreachable')}</p>
      {onRetry && (
        <button type="button" onClick={onRetry} className="btn-primary mt-7">
          {t('common.retry')}
        </button>
      )}
    </div>
  );
}

interface DetailRowProps {
  label: string;
  children: React.ReactNode;
  className?: string;
}

export function DetailRow({ label, children, className }: DetailRowProps) {
  return (
    <div className={cn('detail-row', className)}>
      <div className="detail-label">{label}</div>
      <div className="detail-value">{children}</div>
    </div>
  );
}

interface PanelProps {
  title?: string;
  children: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}

export function Panel({ title, children, actions, className }: PanelProps) {
  return (
    <section className={cn('card overflow-hidden', className)}>
      {(title || actions) && (
        <header className="flex items-center justify-between border-b border-border px-5 py-3.5 sm:px-6">
          {title && <SectionLabel>{title}</SectionLabel>}
          {actions}
        </header>
      )}
      <div className="px-5 py-2 sm:px-6">{children}</div>
    </section>
  );
}

/** Small status dot reflecting the WebSocket connection, in the mono label register. */
export function LiveIndicator({ isConnected, className }: { isConnected: boolean; className?: string }) {
  const { t } = useT();
  return (
    <span className={cn('inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.08em] text-mute', className)}>
      <span className={cn('status-dot', isConnected ? 'status-dot-live' : 'status-dot-off')} />
      {isConnected ? t('common.live') : t('common.offline')}
    </span>
  );
}
