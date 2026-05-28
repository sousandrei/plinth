import { cn } from '@/lib/util';

interface ToggleOption<T extends string> {
  value: T;
  label: string;
}

interface ToggleProps<T extends string> {
  options: [ToggleOption<T>, ToggleOption<T>];
  value: T;
  onValueChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
}

export function Toggle<T extends string>({
  options,
  value,
  onValueChange,
  disabled = false,
  className,
}: ToggleProps<T>): React.JSX.Element {
  return (
    <fieldset
      className={cn(
        'inline-flex border border-border-subtle',
        disabled && 'opacity-50 pointer-events-none',
        className,
      )}
    >
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onValueChange(opt.value)}
            disabled={disabled}
            className={cn(
              'px-4 py-1.5 text-[10px] font-mono uppercase tracking-widest font-bold transition-colors duration-100 outline-none cursor-pointer select-none',
              active
                ? 'bg-foreground text-canvas'
                : 'bg-canvas text-foreground hover:bg-canvas-raised',
            )}
          >
            {opt.label}
          </button>
        );
      })}
    </fieldset>
  );
}
