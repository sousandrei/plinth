import { useMutation, useQueryClient } from '@tanstack/react-query';

import { updateTransaction } from '@/api/transactions';
import { Checkbox } from '@/components/ui/Checkbox';
import { Select } from '@/components/ui/Select';
import { Switch } from '@/components/ui/Switch';
import { categoryChipStyle } from '@/lib/category-color';
import { cn } from '@/lib/util';
import type { Transaction } from '@/types';

// Amounts are stored as minor units (cents ×100). Format to 2 decimal places.
export const formatAmount = (amount: number, currency: string): string => {
  const value = amount / 100;
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
  }).format(value);
};

interface TransactionRowProps {
  transaction: Transaction;
  categories: string[];
  selected: boolean;
  onToggleSelect: (id: string) => void;
}

export const TransactionRow = ({
  transaction: t,
  categories,
  selected,
  onToggleSelect,
}: TransactionRowProps): React.JSX.Element => {
  const isPositive = t.amount >= 0;
  const queryClient = useQueryClient();

  const categoryMutation = useMutation({
    mutationFn: ({ category }: { category: string | null }) =>
      updateTransaction(t.id, t.approved, t.note, category),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['categories'] });
    },
  });

  const approveMutation = useMutation({
    mutationFn: () => updateTransaction(t.id, !t.approved, t.note, t.category),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
    },
  });

  const handleCategoryChange = (val: string | null | undefined) => {
    const category = !val || val === '__clear__' ? null : val;
    categoryMutation.mutate({ category });
  };

  const rowBg = !t.approved
    ? 'bg-highlight-muted/25 hover:bg-highlight-muted/40'
    : 'hover:bg-muted/40';

  return (
    <tr
      className={cn(
        'group border-b border-border-subtle last:border-0 transition-colors duration-100',
        rowBg,
      )}
    >
      {/* Select */}
      <td className="px-6 py-3 w-[40px] min-w-[40px] text-center">
        <Checkbox
          checked={selected}
          onCheckedChange={() => onToggleSelect(t.id)}
          label={`Select transaction ${t.id}`}
        />
      </td>
      {/* Approve */}
      <td className="px-6 py-3 w-[90px] min-w-[90px] text-center">
        <Switch
          checked={t.approved}
          onCheckedChange={() => approveMutation.mutate()}
          disabled={approveMutation.isPending}
          label={t.approved ? 'Unapprove' : 'Approve'}
        />
      </td>

      {/* Date */}
      <td className="px-6 py-3 whitespace-nowrap w-[15%] min-w-[120px]">
        <span className="text-xs font-mono text-muted-foreground">
          {t.value_date}
        </span>
      </td>

      {/* Description */}
      <td className="px-6 py-3 max-w-[320px] w-[40%] min-w-[240px]">
        <p className="text-sm truncate">{t.text}</p>
        {t.note && (
          <p className="text-xs font-mono text-muted-foreground truncate mt-0.5">
            {t.note}
          </p>
        )}
      </td>

      {/* Category Dropdown */}
      <td className="px-6 py-2 whitespace-nowrap w-[24%] min-w-[160px]">
        <Select
          value={t.category ?? '__clear__'}
          onValueChange={(val) => handleCategoryChange(val)}
          disabled={categoryMutation.isPending}
          options={[
            { value: '__clear__', label: 'NONE' },
            ...categories.map((cat) => ({
              value: cat,
              label: cat.toUpperCase(),
            })),
          ]}
          style={t.category ? categoryChipStyle(t.category) : undefined}
          className={cn(
            'h-7 py-0 px-2 text-[10px] font-mono uppercase tracking-widest bg-transparent border shadow-none hover:bg-muted/60 cursor-pointer outline-none transition-all duration-100',
            t.category
              ? 'border-transparent text-foreground'
              : 'border-border-subtle border-dashed text-muted-foreground',
            categoryMutation.isPending && 'opacity-50 pointer-events-none',
          )}
        />
      </td>

      {/* Amount */}
      <td className="px-6 py-3 whitespace-nowrap text-right w-[16%] min-w-[130px]">
        <span
          className={cn(
            'text-sm font-mono font-medium tabular-nums',
            isPositive ? 'text-growth' : 'text-expense',
          )}
        >
          {isPositive ? '+' : ''}
          {formatAmount(t.amount, t.currency)}
        </span>
      </td>
    </tr>
  );
};
