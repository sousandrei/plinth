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
        aria-hidden="true"
        className={cn('w-px self-stretch bg-border-muted', className)}
        {...(props as HTMLAttributes<HTMLDivElement>)}
      />
    );
  }

  return (
    <hr
      className={cn('border-0 h-px w-full bg-border-muted', className)}
      {...(props as HTMLAttributes<HTMLHRElement>)}
    />
  );
};
