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
        'relative inline-flex border border-border-subtle',
        disabled && 'opacity-50 pointer-events-none',
        className,
      )}
    >
      <span
        className="absolute top-0 bottom-0 w-1/2 bg-foreground transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)]"
        style={{
          transform:
            value === options[0].value ? 'translateX(0)' : 'translateX(100%)',
        }}
        aria-hidden="true"
      />
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onValueChange(opt.value)}
            disabled={disabled}
            className={cn(
              'relative z-10 px-4 py-1.5 text-[10px] font-mono uppercase tracking-widest font-bold transition-colors duration-200 outline-none cursor-pointer select-none',
              active ? 'text-canvas' : 'text-foreground hover:bg-muted/50',
            )}
          >
            {opt.label}
          </button>
        );
      })}
    </fieldset>
  );
}
