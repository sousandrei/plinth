import { Popover } from '@base-ui/react/popover';
import {
  CalendarIcon,
  CaretLeftIcon,
  CaretRightIcon,
  X,
} from '@phosphor-icons/react';
import { useState } from 'react';

import { cn } from '@/lib/util';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface DatePickerProps {
  value: string; // ISO "YYYY-MM-DD" or ""
  onChange: (v: string) => void;
  placeholder?: string;
  className?: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

const DAYS = ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'] as const;
const MONTHS = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
] as const;

function parseIso(iso: string): Date | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(iso)) return null;
  const d = new Date(`${iso}T00:00:00`);
  return Number.isNaN(d.getTime()) ? null : d;
}

function toIso(d: Date): string {
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${mm}-${dd}`;
}

function startOfMonth(year: number, month: number): Date {
  return new Date(year, month, 1);
}

function calendarDays(year: number, month: number): (Date | null)[] {
  const first = startOfMonth(year, month);
  const prefix = first.getDay(); // 0=Sun
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const cells: (Date | null)[] = [];
  for (let i = 0; i < prefix; i++) cells.push(null);
  for (let d = 1; d <= daysInMonth; d++) cells.push(new Date(year, month, d));
  while (cells.length % 7 !== 0) cells.push(null);
  return cells;
}

// ─── CalendarGrid ─────────────────────────────────────────────────────────────

interface CalendarGridProps {
  year: number;
  month: number;
  selected: Date | null;
  today: Date;
  onSelect: (iso: string) => void;
  onPrev: () => void;
  onNext: () => void;
}

function CalendarGrid({
  year,
  month,
  selected,
  today,
  onSelect,
  onPrev,
  onNext,
}: CalendarGridProps): React.JSX.Element {
  const cells = calendarDays(year, month);
  const selIso = selected ? toIso(selected) : '';
  const todayIso = toIso(today);

  return (
    <div className="w-[252px] select-none">
      {/* Month / year nav */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border-muted">
        <button
          type="button"
          onClick={onPrev}
          className="h-6 w-6 flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-muted transition-colors duration-100 cursor-pointer"
          aria-label="Previous month"
        >
          <CaretLeftIcon size={12} weight="bold" />
        </button>
        <span className="text-xs font-mono font-bold uppercase tracking-widest">
          {MONTHS[month]} {year}
        </span>
        <button
          type="button"
          onClick={onNext}
          className="h-6 w-6 flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-muted transition-colors duration-100 cursor-pointer"
          aria-label="Next month"
        >
          <CaretRightIcon size={12} weight="bold" />
        </button>
      </div>

      {/* Day labels */}
      <div className="grid grid-cols-7 px-2 pt-2">
        {DAYS.map((d) => (
          <div
            key={d}
            className="h-8 flex items-center justify-center text-[10px] font-mono font-bold uppercase tracking-wider text-muted-foreground"
          >
            {d}
          </div>
        ))}
      </div>

      {/* Day cells */}
      <div className="grid grid-cols-7 px-2 pb-2">
        {cells.map((date, i) => {
          if (!date) {
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed grid layout, stable
            return <div key={i} className="h-8" />;
          }
          const iso = toIso(date);
          const isSelected = iso === selIso;
          const isToday = iso === todayIso;
          const isCurrentMonth = date.getMonth() === month;

          return (
            <button
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed grid layout, stable
              key={i}
              type="button"
              onClick={() => onSelect(iso)}
              className={cn(
                'h-8 w-full flex items-center justify-center text-xs font-mono transition-colors duration-100 cursor-pointer',
                isSelected
                  ? 'bg-foreground text-canvas font-bold'
                  : isToday
                    ? 'ring-1 ring-inset ring-accent text-foreground hover:bg-muted'
                    : isCurrentMonth
                      ? 'text-foreground hover:bg-muted'
                      : 'text-border-muted hover:bg-muted',
              )}
            >
              {date.getDate()}
            </button>
          );
        })}
      </div>
    </div>
  );
}

// ─── DatePicker ───────────────────────────────────────────────────────────────

export function DatePicker({
  value,
  onChange,
  placeholder = 'YYYY-MM-DD',
  className,
}: DatePickerProps): React.JSX.Element {
  const [open, setOpen] = useState(false);
  const [inputText, setInputText] = useState(value);

  // Sync inputText when value changes externally
  const [prevValue, setPrevValue] = useState(value);
  if (value !== prevValue) {
    setPrevValue(value);
    setInputText(value);
  }

  const parsed = parseIso(value);
  const today = new Date();

  // View month defaults to selected date or today
  const viewBase = parsed ?? today;
  const [viewYear, setViewYear] = useState(viewBase.getFullYear());
  const [viewMonth, setViewMonth] = useState(viewBase.getMonth());

  const handleTextChange = (raw: string) => {
    setInputText(raw);
    if (/^\d{4}-\d{2}-\d{2}$/.test(raw)) {
      const d = parseIso(raw);
      if (d) {
        onChange(raw);
        setViewYear(d.getFullYear());
        setViewMonth(d.getMonth());
      }
    } else if (raw === '') {
      onChange('');
    }
  };

  const handleSelect = (iso: string) => {
    onChange(iso);
    setInputText(iso);
    setOpen(false);
  };

  const handlePrev = () => {
    if (viewMonth === 0) {
      setViewMonth(11);
      setViewYear((y) => y - 1);
    } else {
      setViewMonth((m) => m - 1);
    }
  };

  const handleNext = () => {
    if (viewMonth === 11) {
      setViewMonth(0);
      setViewYear((y) => y + 1);
    } else {
      setViewMonth((m) => m + 1);
    }
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger
        className={cn(
          'group inline-flex items-center gap-2 h-10 px-3 w-fit',
          'bg-canvas border border-border-muted text-sm font-mono',
          'focus-within:border-accent focus-within:shadow-[0_0_0_2px_var(--color-accent-muted)]',
          'transition-all duration-150',
          className,
        )}
        render={
          <div>
            <input
              type="text"
              value={inputText}
              onChange={(e) => handleTextChange(e.target.value)}
              placeholder={placeholder}
              maxLength={10}
              className="w-24 bg-transparent outline-none placeholder:text-muted-foreground text-foreground"
            />
            <span className="text-muted-foreground group-focus-within:text-accent transition-colors duration-100">
              {value ? (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onChange('');
                    setInputText('');
                  }}
                  className="flex items-center cursor-pointer hover:text-foreground transition-colors duration-100"
                  aria-label="Clear date"
                >
                  <X size={13} weight="bold" aria-hidden="true" />
                </button>
              ) : (
                <CalendarIcon size={13} weight="regular" aria-hidden="true" />
              )}
            </span>
          </div>
        }
      />
      <Popover.Portal>
        <Popover.Positioner sideOffset={4} align="start">
          <Popover.Popup
            className={cn(
              'bg-canvas-raised border border-border-muted',
              'shadow-[0_8px_24px_-4px_oklch(0%_0_0_/_0.12)]',
              'data-[starting-style]:opacity-0 data-[starting-style]:translate-y-1',
              'transition-[opacity,transform] duration-150',
            )}
          >
            <CalendarGrid
              year={viewYear}
              month={viewMonth}
              selected={parsed}
              today={today}
              onSelect={handleSelect}
              onPrev={handlePrev}
              onNext={handleNext}
            />
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
