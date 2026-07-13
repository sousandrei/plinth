import { useLayoutEffect, useRef, useState } from 'react';
import { cn } from '@/lib/util';

interface ToggleOption<T extends string> {
  value: T;
  label: string;
}

interface ToggleProps<T extends string> {
  options: ToggleOption<T>[];
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
  const activeIndex = Math.max(
    0,
    options.findIndex((opt) => opt.value === value),
  );

  const btnRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [indicator, setIndicator] = useState({ x: 0, w: 0 });

  useLayoutEffect(() => {
    const btn = btnRefs.current[activeIndex];
    if (btn) setIndicator({ x: btn.offsetLeft, w: btn.offsetWidth });
  }, [activeIndex]);

  return (
    <fieldset
      className={cn(
        'relative inline-flex border border-border-subtle',
        disabled && 'opacity-50 pointer-events-none',
        className,
      )}
    >
      <span
        className="absolute top-0 bottom-0 bg-foreground transition-[transform,width] duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)]"
        style={{
          width: indicator.w,
          transform: `translateX(${indicator.x}px)`,
        }}
        aria-hidden="true"
      />
      {options.map((opt, i) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            ref={(el) => {
              btnRefs.current[i] = el;
            }}
            type="button"
            onClick={() => onValueChange(opt.value)}
            disabled={disabled}
            className={cn(
              'relative z-10 h-10 px-4 text-xs font-mono uppercase tracking-widest font-bold transition-colors duration-200 outline-none cursor-pointer select-none',
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
