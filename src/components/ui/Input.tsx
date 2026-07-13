import type { InputHTMLAttributes } from 'react';
import { cn } from '@/lib/util';

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {}

export const Input = ({
  className,
  ...props
}: InputProps): React.JSX.Element => (
  <input
    className={cn(
      'w-full h-10 px-4 text-sm font-sans',
      'bg-canvas-raised text-foreground',
      'border border-border-subtle',
      'shadow-[0_1px_2px_0_oklch(0%_0_0_/_0.04)]',
      'transition-all duration-150',
      'hover:border-border-muted',
      'focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
      'placeholder:text-muted-foreground',
      'disabled:opacity-50 disabled:pointer-events-none',
      className,
    )}
    {...props}
  />
);
