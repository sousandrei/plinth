import type { ButtonHTMLAttributes } from 'react';
import { cn } from '@/lib/util';

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost';
}

export const Button = ({
  variant = 'primary',
  className,
  children,
  ...props
}: ButtonProps): React.JSX.Element => (
  <button
    type="button"
    className={cn(
      'inline-flex items-center justify-center px-5 py-2.5 text-sm font-medium',
      'transition-all duration-150 cursor-pointer',
      'active:scale-[0.97]',
      'disabled:pointer-events-none disabled:opacity-50',
      'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-1',
      variant === 'primary' && [
        'bg-foreground text-canvas border border-foreground',
        'hover:bg-border hover:shadow-[0_2px_8px_-1px_oklch(0%_0_0_/_0.25)]',
      ],
      variant === 'secondary' && [
        'bg-canvas-raised text-foreground border border-border-subtle',
        'shadow-[0_1px_2px_0_oklch(0%_0_0_/_0.04)]',
        'hover:border-border-muted hover:shadow-[0_2px_6px_-1px_oklch(0%_0_0_/_0.08)]',
      ],
      variant === 'ghost' && [
        'bg-transparent text-foreground border border-transparent',
        'hover:border-border-subtle hover:bg-muted',
      ],
      className,
    )}
    {...props}
  >
    {children}
  </button>
);
