import type { HTMLAttributes, TdHTMLAttributes, ThHTMLAttributes } from 'react';
import { cn } from '@/lib/util';

export const Table = ({
  className,
  ...props
}: HTMLAttributes<HTMLTableElement>): React.JSX.Element => (
  <div className="w-full overflow-x-auto">
    <table
      className={cn('w-full text-sm border-collapse', className)}
      {...props}
    />
  </div>
);

export const TableHead = ({
  className,
  ...props
}: HTMLAttributes<HTMLTableSectionElement>): React.JSX.Element => (
  <thead
    className={cn(
      'bg-[linear-gradient(to_bottom,var(--color-muted),var(--color-canvas-raised))]',
      className,
    )}
    {...props}
  />
);

export const TableBody = ({
  className,
  ...props
}: HTMLAttributes<HTMLTableSectionElement>): React.JSX.Element => (
  <tbody
    className={cn('divide-y divide-border-subtle', className)}
    {...props}
  />
);

export const TableRow = ({
  className,
  ...props
}: HTMLAttributes<HTMLTableRowElement>): React.JSX.Element => (
  <tr
    className={cn(
      'relative transition-colors duration-150',
      'odd:bg-canvas even:bg-canvas-raised',
      'hover:bg-accent-muted',
      '[&:hover_td]:border-l-accent [&:hover_td:first-child]:border-l-2',
      className,
    )}
    {...props}
  />
);

export const Th = ({
  className,
  ...props
}: ThHTMLAttributes<HTMLTableCellElement>): React.JSX.Element => (
  <th
    className={cn(
      'px-4 py-3 text-left text-xs uppercase tracking-widest font-mono',
      'text-muted-foreground font-medium',
      'border-b border-border-muted',
      className,
    )}
    {...props}
  />
);

export const Td = ({
  className,
  ...props
}: TdHTMLAttributes<HTMLTableCellElement>): React.JSX.Element => (
  <td
    className={cn(
      'px-4 py-3 text-sm text-foreground border-l border-l-transparent transition-colors duration-150',
      className,
    )}
    {...props}
  />
);

export const TdMono = ({
  className,
  ...props
}: TdHTMLAttributes<HTMLTableCellElement>): React.JSX.Element => (
  <td
    className={cn(
      'px-4 py-3 text-sm font-mono tabular-nums text-foreground border-l border-l-transparent transition-colors duration-150',
      className,
    )}
    {...props}
  />
);
