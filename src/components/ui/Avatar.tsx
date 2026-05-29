import { cn } from '@/lib/util';

interface AvatarProps {
  name: string;
  className?: string;
  onClick?: () => void;
}

export const Avatar = ({
  name,
  className,
  onClick,
}: AvatarProps): React.JSX.Element => (
  <button
    type="button"
    onClick={onClick}
    className={cn(
      'w-8 h-8 rounded-full shrink-0',
      'bg-foreground text-canvas',
      'text-[11px] font-semibold uppercase tracking-wide',
      'flex items-center justify-center',
      'transition-opacity duration-150 hover:opacity-80',
      'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-1',
      'cursor-pointer',
      className,
    )}
  >
    {name.slice(0, 2)}
  </button>
);
