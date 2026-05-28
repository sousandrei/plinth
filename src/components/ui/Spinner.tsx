import { cn } from '@/lib/util';

interface SpinnerProps {
  className?: string;
  size?: 'sm' | 'md' | 'lg';
}

const sizes = {
  sm: 'h-4 w-4',
  md: 'h-6 w-6',
  lg: 'h-8 w-8',
};

export const Spinner = ({
  size = 'md',
  className,
}: SpinnerProps): React.JSX.Element => (
  <span
    role="status"
    aria-label="Loading"
    className={cn(
      'inline-block animate-spin border border-border-muted border-t-foreground',
      sizes[size],
      className,
    )}
  />
);
