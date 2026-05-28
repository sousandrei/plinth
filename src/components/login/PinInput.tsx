import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/util';

interface PinInputProps {
  length?: number;
  onComplete: (pin: string) => void;
  error?: string;
  disabled?: boolean;
}

export const PinInput = ({
  length = 4,
  onComplete,
  error,
  disabled,
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

  const handleKey = (i: number, e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Backspace') {
      e.preventDefault();
      const next = [...digits];
      if (next[i]) {
        next[i] = '';
        setDigits(next);
      } else if (i > 0) {
        next[i - 1] = '';
        setDigits(next);
        refs.current[i - 1]?.focus();
      }
    }
  };

  const handleChange = (i: number, val: string) => {
    const digit = val.replace(/\D/g, '').slice(-1);
    const next = [...digits];
    next[i] = digit;
    setDigits(next);

    if (digit && i < length - 1) {
      refs.current[i + 1]?.focus();
    }

    if (digit && i === length - 1) {
      const pin = next.join('');
      if (pin.length === length) onComplete(pin);
    }
  };

  return (
    <div className="flex flex-col items-center gap-4">
      <div className="flex gap-3">
        {digits.map((d, i) => (
          <input
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length PIN array
            key={i}
            ref={(el) => {
              refs.current[i] = el;
            }}
            type="password"
            inputMode="numeric"
            maxLength={1}
            value={d}
            disabled={disabled}
            onChange={(e) => handleChange(i, e.target.value)}
            onKeyDown={(e) => handleKey(i, e)}
            onFocus={(e) => e.target.select()}
            className={cn(
              'w-12 h-14 text-center text-xl font-mono',
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
