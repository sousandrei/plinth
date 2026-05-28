import type { HTMLAttributes } from 'react';
import { cn } from '@/lib/util';

interface SeparatorProps extends HTMLAttributes<HTMLElement> {
  orientation?: 'horizontal' | 'vertical';
}

export const Separator = ({
  orientation = 'horizontal',
  className,
  ...props
}: SeparatorProps): React.JSX.Element => {
  if (orientation === 'vertical') {
    return (
      <div
        className={cn(
          'w-px self-stretch bg-[linear-gradient(to_bottom,transparent,var(--color-border-muted)_20%,var(--color-border-muted)_80%,transparent)]',
          className,
        )}
        {...(props as HTMLAttributes<HTMLDivElement>)}
      />
    );
  }

  return (
    <hr
      className={cn(
        'border-0 h-px w-full bg-[linear-gradient(to_right,transparent,var(--color-border-muted)_20%,var(--color-border-muted)_80%,transparent)]',
        className,
      )}
      {...(props as HTMLAttributes<HTMLHRElement>)}
    />
  );
};
