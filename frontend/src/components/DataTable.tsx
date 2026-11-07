'use client';

import { useT } from '@/i18n/client';
import { cn } from '@/lib/utils';

export interface Column<T> {
  key: string;
  header: string;
  className?: string;
  render: (item: T, index: number) => React.ReactNode;
}

interface DataTableProps<T> {
  columns: Column<T>[];
  data: T[];
  keyExtractor: (item: T) => string;
  onRowClick?: (item: T) => void;
  isLoading?: boolean;
  emptyMessage?: string;
  className?: string;
  /** `card` (default): a bordered surface. `ruled`: no box, a heavy rule on top and row rules. */
  variant?: 'card' | 'ruled';
}

export function DataTable<T>({
  columns,
  data,
  keyExtractor,
  onRowClick,
  isLoading,
  emptyMessage,
  className,
  variant = 'card',
}: DataTableProps<T>) {
  const { t } = useT();
  const frame = variant === 'ruled' ? 'border-y border-y-border border-t-ink' : 'card';
  if (isLoading) {
    return <DataTableSkeleton columns={columns.length} rows={5} className={variant === 'ruled' ? frame : undefined} />;
  }

  if (data.length === 0) {
    return (
      <div className={cn(frame, className)}>
        <div className="flex h-40 items-center justify-center">
          <p className="text-sm text-mute">{emptyMessage ?? t('components.dataTable.empty')}</p>
        </div>
      </div>
    );
  }

  return (
    <div className={cn(frame, 'overflow-hidden', className)}>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse">
          <thead>
            <tr className="border-b border-border">
              {columns.map((column) => (
                <th
                  key={column.key}
                  className={cn('table-head', column.className)}
                >
                  {column.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.map((item, index) => (
              <tr
                key={keyExtractor(item)}
                onClick={() => onRowClick?.(item)}
                className={cn(
                  'border-b border-border-soft transition-colors last:border-0 hover:bg-[var(--hover)]',
                  onRowClick && 'cursor-pointer'
                )}
              >
                {columns.map((column) => (
                  <td
                    key={column.key}
                    className={cn('whitespace-nowrap px-4 py-3 text-sm text-text', column.className)}
                  >
                    {column.render(item, index)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

interface DataTableSkeletonProps {
  columns: number;
  rows: number;
  className?: string;
}

export function DataTableSkeleton({ columns, rows, className }: DataTableSkeletonProps) {
  return (
    <div className={cn(className ?? 'card', 'overflow-hidden')}>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse">
          <thead>
            <tr className="border-b border-border">
              {Array.from({ length: columns }).map((_, i) => (
                <th key={i} className="px-4 py-2.5">
                  <div className="h-3 w-16 animate-pulse bg-bg-soft" />
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: rows }).map((_, rowIndex) => (
              <tr key={rowIndex} className="border-b border-border-soft last:border-0">
                {Array.from({ length: columns }).map((_, colIndex) => (
                  <td key={colIndex} className="px-4 py-3">
                    <div className="h-3.5 w-24 animate-pulse bg-bg-soft" />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
