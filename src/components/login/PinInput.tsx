import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/util';

interface PinInputProps {
  length?: number;
  onComplete: (pin: string) => void;
  error?: string;
  disabled?: boolean;
  visible?: boolean;
}

export const PinInput = ({
  length = 4,
  onComplete,
  error,
  disabled,
  visible = false,
}: PinInputProps): React.JSX.Element => {
  const [digits, setDigits] = useState<string[]>(Array(length).fill(''));
  const refs = useRef<(HTMLInputElement | null)[]>([]);

  useEffect(() => {
    refs.current[0]?.focus();
  }, []);

  useEffect(() => {
    if (error) {
      setDigits(Array(length).fill(''));
      refs.current[0]?.focus();
    }
  }, [error, length]);

  const updateDigits = (next: string[]) => {
    setDigits(next);
    if (next.every(Boolean)) onComplete(next.join(''));
  };

  const handleKey = (i: number, e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Backspace') {
      e.preventDefault();
      const next = [...digits];
      if (next[i]) {
        next[i] = '';
        updateDigits(next);
      } else if (i > 0) {
        next[i - 1] = '';
        updateDigits(next);
        refs.current[i - 1]?.focus();
      }
    }
  };

  const handleChange = (i: number, val: string) => {
    const digit = val.replace(/\D/g, '').slice(-1);
    const next = [...digits];
    next[i] = digit;
    updateDigits(next);

    if (digit && i < length - 1) {
      refs.current[i + 1]?.focus();
    }
  };

  return (
    <div className="flex flex-col items-center gap-4">
      <div className={cn('flex', length > 4 ? 'gap-2' : 'gap-3')}>
        {digits.map((d, i) => (
          <input
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length PIN array
            key={i}
            ref={(el) => {
              refs.current[i] = el;
            }}
            type={visible ? 'text' : 'password'}
            aria-label={`Digit ${i + 1}`}
            autoComplete="off"
            inputMode="numeric"
            maxLength={1}
            value={d}
            disabled={disabled}
            onChange={(e) => handleChange(i, e.target.value)}
            onKeyDown={(e) => handleKey(i, e)}
            onFocus={(e) => e.target.select()}
            onPaste={(event) => {
              event.preventDefault();
              const pasted = event.clipboardData
                .getData('text')
                .replace(/\D/g, '')
                .slice(0, length - i);
              if (!pasted) return;
              const next = [...digits];
              next.splice(i, pasted.length, ...pasted);
              updateDigits(next);
              refs.current[Math.min(i + pasted.length, length - 1)]?.focus();
            }}
            className={cn(
              'text-center text-xl font-mono',
              length > 4 ? 'size-9 sm:w-12 sm:h-14' : 'w-12 h-14',
              'bg-canvas border border-border-muted',
              'transition-all duration-150',
              'focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
              error && 'border-expense',
              disabled && 'opacity-40 cursor-not-allowed',
            )}
          />
        ))}
      </div>
      {error && (
        <p className="text-xs font-mono text-expense animate-fade-in">
          {error}
        </p>
      )}
    </div>
  );
};
