import type { HTMLAttributes } from 'react';
import { cn } from '@/lib/util';

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: 'default' | 'muted' | 'growth' | 'expense' | 'highlight' | 'accent';
}

export const Badge = ({
  variant = 'default',
  className,
  children,
  ...props
}: BadgeProps): React.JSX.Element => (
  <span
    className={cn(
      'inline-flex items-center px-2 py-0.5 text-xs uppercase tracking-widest font-mono border',
      'transition-colors duration-150',
      variant === 'default' && 'bg-foreground text-canvas border-foreground',
      variant === 'muted' &&
        'bg-muted text-muted-foreground border-border-muted',
      variant === 'accent' && 'bg-accent-muted text-accent border-accent/30',
      variant === 'growth' && 'bg-growth-muted text-growth border-growth/30',
      variant === 'expense' &&
        'bg-expense-muted text-expense border-expense/30',
      variant === 'highlight' &&
        'bg-highlight-muted text-highlight border-highlight/30',
      className,
    )}
    {...props}
  >
    {children}
  </span>
);
