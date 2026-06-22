import { Check } from '@phosphor-icons/react';

import { cn } from '@/lib/util';

interface CheckboxProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  label?: string;
  id?: string;
  indeterminate?: boolean;
}

export const Checkbox = ({
  checked,
  onCheckedChange,
  disabled = false,
  label,
  id,
  indeterminate = false,
}: CheckboxProps): React.JSX.Element => {
  const isActive = checked || indeterminate;
  return (
    <span className="relative inline-flex h-4 w-4 shrink-0 items-center justify-center">
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-label={label}
        ref={(el) => {
          if (el) el.indeterminate = indeterminate;
        }}
        onChange={(e) => onCheckedChange(e.target.checked)}
        className={cn(
          'peer absolute inset-0 h-4 w-4 cursor-pointer appearance-none border transition-colors duration-150',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-foreground focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
          isActive
            ? 'bg-foreground border-foreground'
            : 'bg-canvas border-border-muted hover:border-foreground',
          disabled && 'opacity-40 pointer-events-none',
        )}
      />
      {checked && (
        <Check
          weight="bold"
          aria-hidden="true"
          size={12}
          className="pointer-events-none absolute text-canvas"
        />
      )}
      {indeterminate && !checked && (
        <span
          className="pointer-events-none absolute h-[2px] w-2 bg-canvas"
          aria-hidden="true"
        />
      )}
    </span>
  );
};
