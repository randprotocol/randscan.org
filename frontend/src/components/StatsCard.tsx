import { cn } from '@/lib/utils';

interface StatsCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  icon?: React.ReactNode;
  trend?: {
    value: number;
    isPositive: boolean;
  };
  className?: string;
}

/**
 * A single cell of a hairline-separated stat row: mono label, big tabular
 * figure, smaller muted sub-label. Wrap a group in <StatsRow> to get the rules.
 */
export function StatsCard({ title, value, subtitle, trend, className }: StatsCardProps) {
  return (
    <div className={cn('stat', className)}>
      <p className="stat-label">{title}</p>
      <p className="stat-value">{value}</p>
      {subtitle && <p className="stat-sub">{subtitle}</p>}
      {trend && (
        <p className={cn('stat-sub', trend.isPositive ? 'text-positive' : 'text-negative')}>
          {trend.isPositive ? '+' : '−'}
          {Math.abs(trend.value)}%
        </p>
      )}
    </div>
  );
}

interface StatsRowProps {
  children: React.ReactNode;
  /** Number of columns on large screens; 3 and 4 stack below `sm`, 5 is two-up on phones. */
  columns?: 3 | 4 | 5;
  className?: string;
}

// An odd last cell in a two-column layout spans the row, so no empty cell shows the rule colour.
const columnClasses: Record<number, string> = {
  3: 'grid-cols-1 sm:grid-cols-3',
  4: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-4',
  5: 'grid-cols-2 lg:grid-cols-5 [&>*:last-child:nth-child(odd)]:col-span-2 lg:[&>*:last-child:nth-child(odd)]:col-span-1',
};

export function StatsRow({ children, columns = 4, className }: StatsRowProps) {
  return <div className={cn('stat-row', columnClasses[columns], className)}>{children}</div>;
}

export function StatsCardSkeleton() {
  return (
    <div className="stat">
      <div className="h-3 w-20 animate-pulse bg-bg-soft" />
      <div className="mt-3 h-8 w-28 animate-pulse bg-bg-soft" />
    </div>
  );
}
