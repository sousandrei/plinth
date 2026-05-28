import { DatePicker } from '@/components/ui/DatePicker';
import { Select } from '@/components/ui/Select';
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
  'h-8 px-3 text-xs font-mono bg-canvas border border-border-muted',
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
      />

      {/* Approved toggle */}
      <div className="flex border border-border-muted">
        {(['all', 'approved', 'unapproved'] as const).map((val) => (
          <button
            key={val}
            type="button"
            onClick={() => set({ approved: val })}
            className={cn(
              'h-8 px-3 text-xs font-mono uppercase tracking-wider transition-colors duration-100',
              filters.approved === val
                ? 'bg-foreground text-canvas'
                : 'bg-canvas text-muted-foreground hover:bg-muted',
            )}
          >
            {val}
          </button>
        ))}
      </div>

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
        className="h-8 py-0 px-3 text-xs font-mono uppercase tracking-wider w-52 bg-canvas border border-border-muted"
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
        <button
          type="button"
          onClick={() =>
            onChange({
              search: '',
              approved: 'all',
              category: 'all',
              dateFrom: '',
              dateTo: '',
            })
          }
          className="h-8 px-3 text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-100 underline underline-offset-4"
        >
          Clear
        </button>
      )}
    </div>
  );
};
