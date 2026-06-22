import { Select as BaseSelect } from '@base-ui/react/select';
import { CaretDown } from '@phosphor-icons/react';
import type { ReactNode } from 'react';
import { cn } from '@/lib/util';

interface SelectOption {
  value: string;
  label: string;
}

interface SelectProps {
  value?: string;
  onValueChange?: (value: string | null) => void;
  options: SelectOption[];
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  style?: React.CSSProperties;
}

const ChevronIcon = (): React.JSX.Element => (
  <CaretDown
    size={10}
    weight="bold"
    aria-hidden="true"
    className="shrink-0 transition-transform duration-200 group-data-[open]:rotate-180"
  />
);

export const Select = ({
  value,
  onValueChange,
  options,
  placeholder = 'Select…',
  className,
  disabled,
  style,
}: SelectProps): React.JSX.Element => (
  <BaseSelect.Root
    value={value}
    onValueChange={onValueChange}
    disabled={disabled}
  >
    <BaseSelect.Trigger
      style={style}
      className={cn(
        'group inline-flex items-center justify-between gap-2 w-full px-4 py-3 text-sm font-sans',
        'bg-canvas-raised text-foreground',
        'border border-border-subtle',
        'shadow-[0_1px_2px_0_oklch(0%_0_0_/_0.04)]',
        'transition-all duration-150',
        'hover:border-border-muted hover:shadow-[0_1px_4px_0_oklch(0%_0_0_/_0.08)]',
        'focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
        'disabled:opacity-50 disabled:pointer-events-none',
        'data-[placeholder]:text-muted-foreground',
        className,
      )}
    >
      <BaseSelect.Value className="truncate text-left flex-1">
        {(v: string | null) => {
          const selected = options.find((opt) => opt.value === v);
          return selected ? selected.label : placeholder;
        }}
      </BaseSelect.Value>
      <ChevronIcon />
    </BaseSelect.Trigger>
    <BaseSelect.Portal>
      <BaseSelect.Positioner className="z-[60]">
        <BaseSelect.Popup
          className={cn(
            'bg-canvas-raised border border-border-muted py-1',
            'shadow-[0_8px_24px_-4px_oklch(0%_0_0_/_0.12)]',
            'min-w-[var(--anchor-width)]',
          )}
        >
          {options.map((opt) => (
            <SelectItem key={opt.value} value={opt.value}>
              {opt.label}
            </SelectItem>
          ))}
        </BaseSelect.Popup>
      </BaseSelect.Positioner>
    </BaseSelect.Portal>
  </BaseSelect.Root>
);

interface SelectItemProps {
  value: string;
  children: ReactNode;
}

const SelectItem = ({
  value,
  children,
}: SelectItemProps): React.JSX.Element => (
  <BaseSelect.Item
    value={value}
    className={cn(
      'px-4 py-2 text-sm font-sans cursor-pointer truncate',
      'text-foreground transition-colors duration-100',
      'data-[highlighted]:bg-accent-muted data-[highlighted]:text-accent data-[highlighted]:outline-none',
      'data-[selected]:font-semibold data-[selected]:text-accent',
    )}
  >
    {children}
  </BaseSelect.Item>
);
