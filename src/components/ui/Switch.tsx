import { cn } from '@/lib/util';

interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  label?: string;
  id?: string;
}

export const Switch = ({
  checked,
  onCheckedChange,
  disabled = false,
  label,
  id,
}: SwitchProps): React.JSX.Element => {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        'relative inline-flex h-5 w-9 shrink-0 items-center border transition-colors duration-150',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-foreground focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
        'cursor-pointer',
        checked
          ? 'bg-foreground border-foreground'
          : 'bg-canvas border-border-muted hover:border-foreground',
        disabled && 'opacity-40 pointer-events-none',
      )}
    >
      <span
        className={cn(
          'pointer-events-none inline-block h-3 w-3 transition-transform duration-150',
          checked
            ? 'bg-canvas translate-x-[18px]'
            : 'bg-foreground translate-x-1',
        )}
      />
    </button>
  );
};
