import type { HTMLAttributes, ReactNode } from 'react';
import { cn } from '@/lib/util';

interface CardProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

interface CardHeaderProps extends HTMLAttributes<HTMLDivElement> {
  children?: ReactNode;
  label?: string;
  meta?: string;
}

interface CardBodyProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export const Card = ({
  className,
  children,
  ...props
}: CardProps): React.JSX.Element => (
  <div
    className={cn(
      'border border-border-subtle bg-canvas-raised',
      'shadow-[0_1px_3px_0_oklch(0%_0_0_/_0.06)]',
      'transition-all duration-200',
      'hover:border-border-muted hover:shadow-[0_4px_12px_-2px_oklch(0%_0_0_/_0.10)]',
      'hover:-translate-y-px',
      className,
    )}
    {...props}
  >
    {children}
  </div>
);

export const CardHeader = ({
  label,
  meta,
  className,
  children,
  ...props
}: CardHeaderProps): React.JSX.Element => (
  <div
    className={cn(
      'px-6 py-4 border-b border-foreground/10 flex items-center justify-between',
      'bg-foreground',
      className,
    )}
    {...props}
  >
    {label || meta ? (
      <>
        {label && (
          <span className="text-xs uppercase tracking-widest font-mono text-canvas/60">
            {label}
          </span>
        )}
        {meta && (
          <span className="text-xs font-mono text-canvas font-semibold">
            {meta}
          </span>
        )}
      </>
    ) : (
      children
    )}
  </div>
);

export const CardBody = ({
  className,
  children,
  ...props
}: CardBodyProps): React.JSX.Element => (
  <div className={cn('p-6', className)} {...props}>
    {children}
  </div>
);
