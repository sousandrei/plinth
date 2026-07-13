import { cn } from '@/lib/util';

interface PaginationProps {
  page: number;
  pageCount: number;
  onPageChange: (page: number) => void;
  className?: string;
}

export function Pagination({
  page,
  pageCount,
  onPageChange,
  className,
}: PaginationProps): React.JSX.Element | null {
  if (pageCount <= 1) {
    return null;
  }

  return (
    <div
      className={cn(
        'flex items-center justify-between animate-fade-in',
        className,
      )}
    >
      <button
        type="button"
        onClick={() => onPageChange(Math.max(0, page - 1))}
        disabled={page === 0}
        className={cn(
          'h-10 px-4 text-xs font-mono uppercase tracking-wider border border-border-muted',
          'transition-all duration-150',
          'hover:bg-muted disabled:opacity-30 disabled:cursor-not-allowed cursor-pointer',
        )}
      >
        ← Prev
      </button>
      <span className="text-xs font-mono text-muted-foreground">
        {page + 1} / {pageCount}
      </span>
      <button
        type="button"
        onClick={() => onPageChange(Math.min(pageCount - 1, page + 1))}
        disabled={page >= pageCount - 1}
        className={cn(
          'h-10 px-4 text-xs font-mono uppercase tracking-wider border border-border-muted',
          'transition-all duration-150',
          'hover:bg-muted disabled:opacity-30 disabled:cursor-not-allowed cursor-pointer',
        )}
      >
        Next →
      </button>
    </div>
  );
}
