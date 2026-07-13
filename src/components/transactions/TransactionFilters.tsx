import { Button } from '@/components/ui/Button';
import { DatePicker } from '@/components/ui/DatePicker';
import { Select } from '@/components/ui/Select';
import { Toggle } from '@/components/ui/Toggle';
import { cn } from '@/lib/util';

export interface FilterState {
  search: string;
  approved: 'all' | 'approved' | 'unapproved';
  dateFrom: string;
  dateTo: string;
  category: string;
}

interface TransactionFiltersProps {
  filters: FilterState;
  onChange: (filters: FilterState) => void;
  categories: string[];
}

const inputBase = cn(
  'h-10 px-3 text-xs font-mono bg-canvas border border-border-muted',
  'placeholder:text-muted-foreground',
  'focus:outline-none focus:border-accent focus:shadow-[0_0_0_2px_var(--color-accent-muted)]',
  'transition-all duration-150',
);

export const TransactionFilters = ({
  filters,
  onChange,
  categories,
}: TransactionFiltersProps): React.JSX.Element => {
  const set = (patch: Partial<FilterState>) =>
    onChange({ ...filters, ...patch });

  return (
    <div className="flex items-center gap-3 flex-wrap">
      {/* Search */}
      <input
        type="search"
        value={filters.search}
        onChange={(e) => set({ search: e.target.value })}
        placeholder="Search…"
        className={cn(inputBase, 'w-52')}
        autoCorrect="off"
        autoCapitalize="none"
        spellCheck={false}
      />

      {/* Approved toggle */}
      <Toggle
        options={[
          { value: 'all', label: 'All' },
          { value: 'approved', label: 'Approved' },
          { value: 'unapproved', label: 'Unapproved' },
        ]}
        value={filters.approved}
        onValueChange={(val) => set({ approved: val })}
      />

      {/* Category filter */}
      <Select
        value={filters.category}
        onValueChange={(val) => set({ category: val ?? 'all' })}
        options={[
          { value: 'all', label: 'ALL CATEGORIES' },
          ...categories.map((cat) => ({
            value: cat,
            label: cat.toUpperCase(),
          })),
        ]}
        className="h-10 px-3 text-xs font-mono uppercase tracking-wider w-52 bg-canvas border border-border-muted"
      />

      {/* Date range */}
      <DatePicker
        value={filters.dateFrom}
        onChange={(v) => set({ dateFrom: v })}
        placeholder="From…"
      />
      <span className="text-xs font-mono text-muted-foreground">→</span>
      <DatePicker
        value={filters.dateTo}
        onChange={(v) => set({ dateTo: v })}
        placeholder="To…"
      />

      {/* Clear */}
      {(filters.search ||
        filters.approved !== 'all' ||
        filters.category !== 'all' ||
        filters.dateFrom ||
        filters.dateTo) && (
        <Button
          variant="ghost"
          onClick={() =>
            onChange({
              search: '',
              approved: 'all',
              category: 'all',
              dateFrom: '',
              dateTo: '',
            })
          }
        >
          Clear
        </Button>
      )}
    </div>
  );
};
