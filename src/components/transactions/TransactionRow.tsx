import { ArrowClockwiseIcon } from '@phosphor-icons/react';
import { useMutation, useQueryClient } from '@tanstack/react-query';

import { updateTransaction } from '@/api/transactions';
import { classifyTransactions } from '@/api/upload';
import { Checkbox } from '@/components/ui/Checkbox';
import { Select } from '@/components/ui/Select';
import { Switch } from '@/components/ui/Switch';
import { toast } from '@/components/ui/Toast';
import { categoryChipStyle } from '@/lib/category-color';
import { cn } from '@/lib/util';
import type { Transaction, TransactionPage } from '@/types';

// Amounts are stored as minor units (cents ×100). Format to 2 decimal places.
export const formatAmount = (amount: number, currency: string): string => {
  const value = amount / 100;
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
  }).format(value);
};

/// Translate classifier/predict failures into a user-facing toast. The
/// most common cause is "no trained model yet" — show a hint to train
/// one. Other failures fall back to the raw error message.
const describePredictError = (err: unknown): string => {
  const msg = err instanceof Error ? err.message : String(err);
  if (msg.includes('classifier not ready')) {
    return 'Train a model first — the classifier has nothing loaded yet.';
  }
  return msg;
};

interface TransactionRowProps {
  transaction: Transaction;
  categories: string[];
  selected: boolean;
  isDemoMode: boolean;
  onToggleSelect: (id: string) => void;
}

export const TransactionRow = ({
  transaction: t,
  categories,
  selected,
  isDemoMode,
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

  const predictMutation = useMutation({
    mutationFn: async () => {
      const inputs = [
        {
          text: t.text,
          amount: t.amount,
          booking_date: t.booking_date,
        },
      ];
      const [prediction] = await classifyTransactions(inputs);
      if (!prediction) return;
      if (isDemoMode) {
        queryClient.setQueriesData<TransactionPage>(
          { queryKey: ['transactions', 'demo'] },
          (old) => {
            if (!old) return old;
            return {
              ...old,
              transactions: old.transactions.map((row) =>
                row.id === t.id ? { ...row, category: prediction } : row,
              ),
            };
          },
        );
        return;
      }
      await updateTransaction(t.id, t.approved, t.note, prediction);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['categories'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
    },
    onError: (err: unknown) => {
      handlePredictError(err);
    },
  });

  const handleCategoryChange = (val: string | null | undefined) => {
    const category = !val || val === '__clear__' ? null : val;
    categoryMutation.mutate({ category });
  };

  const handlePredictError = (err: unknown): void => {
    toast.error('Prediction failed', describePredictError(err));
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
      <td className="px-6 py-4 w-[40px] min-w-[40px] text-center">
        <Checkbox
          checked={selected}
          onCheckedChange={() => onToggleSelect(t.id)}
          label={`Select transaction ${t.id}`}
        />
      </td>
      {/* Approve */}
      <td className="px-6 py-4 w-[90px] min-w-[90px] text-center">
        <Switch
          checked={t.approved}
          onCheckedChange={() => approveMutation.mutate()}
          disabled={approveMutation.isPending}
          label={t.approved ? 'Unapprove' : 'Approve'}
        />
      </td>

      {/* Date */}
      <td className="px-6 py-4 whitespace-nowrap w-[15%] min-w-[120px]">
        <span className="text-xs font-mono text-muted-foreground">
          {t.value_date}
        </span>
      </td>

      {/* Description */}
      <td className="px-6 py-4 max-w-[320px] w-[40%] min-w-[240px]">
        <p className="text-sm truncate">{t.text}</p>
        {t.note && (
          <p className="text-xs font-mono text-muted-foreground truncate mt-0.5">
            {t.note}
          </p>
        )}
      </td>

      {/* Category Dropdown */}
      <td className="px-6 py-2 whitespace-nowrap w-[24%] min-w-[160px]">
        <div className="flex items-center gap-1.5">
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
              'flex-1 min-w-0 h-7 py-0 px-2 text-[10px] font-mono uppercase tracking-widest bg-transparent border shadow-none hover:bg-muted/60 cursor-pointer outline-none transition-all duration-100',
              t.category
                ? 'border-transparent text-foreground'
                : 'border-border-subtle border-dashed text-muted-foreground',
              categoryMutation.isPending && 'opacity-50 pointer-events-none',
            )}
          />
          <button
            type="button"
            onClick={() => predictMutation.mutate()}
            disabled={predictMutation.isPending}
            aria-label="Predict category"
            title="Predict category"
            className={cn(
              'shrink-0 w-7 h-7 flex items-center justify-center text-muted-foreground hover:text-foreground border border-border-subtle hover:bg-muted/60 transition-colors duration-100 disabled:opacity-40 disabled:cursor-not-allowed',
            )}
          >
            <ArrowClockwiseIcon
              size={11}
              weight="bold"
              aria-hidden="true"
              className={predictMutation.isPending ? 'animate-spin' : ''}
            />
          </button>
        </div>
      </td>

      {/* Amount */}
      <td className="px-6 py-4 whitespace-nowrap text-right w-[16%] min-w-[130px]">
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
