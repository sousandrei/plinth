import { useMutation, useQueryClient } from '@tanstack/react-query';

import {
  bulkApproveTransactions,
  bulkCategorizeTransactions,
} from '@/api/transactions';
import { Button } from '@/components/ui/Button';
import { Select } from '@/components/ui/Select';
import type { TransactionPage } from '@/types';

interface BulkActionBarProps {
  selectedIds: string[];
  categories: string[];
  isDemoMode: boolean;
  onClear: () => void;
}

export const BulkActionBar = ({
  selectedIds,
  categories,
  isDemoMode,
  onClear,
}: BulkActionBarProps): React.JSX.Element | null => {
  const queryClient = useQueryClient();
  const count = selectedIds.length;

  const invalidate = (): void => {
    queryClient.invalidateQueries({ queryKey: ['transactions'] });
    queryClient.invalidateQueries({ queryKey: ['aggregations'] });
    queryClient.invalidateQueries({ queryKey: ['categories'] });
  };

  const applyDemoApproved = (approved: boolean): void => {
    queryClient.setQueriesData<TransactionPage>(
      { queryKey: ['transactions', 'demo'] },
      (old) => {
        if (!old) return old;
        return {
          ...old,
          transactions: old.transactions.map((t) =>
            selectedIds.includes(t.id) ? { ...t, approved } : t,
          ),
        };
      },
    );
  };

  const applyDemoCategory = (cat: string | null): void => {
    queryClient.setQueriesData<TransactionPage>(
      { queryKey: ['transactions', 'demo'] },
      (old) => {
        if (!old) return old;
        return {
          ...old,
          transactions: old.transactions.map((t) =>
            selectedIds.includes(t.id) ? { ...t, category: cat ?? '' } : t,
          ),
        };
      },
    );
  };

  const approveMutation = useMutation({
    mutationFn: (approved: boolean) =>
      isDemoMode
        ? Promise.resolve(applyDemoApproved(approved))
        : bulkApproveTransactions(selectedIds, approved).then(() => undefined),
    onSuccess: () => {
      invalidate();
      onClear();
    },
  });

  const categorizeMutation = useMutation({
    mutationFn: (cat: string | null) =>
      isDemoMode
        ? Promise.resolve(applyDemoCategory(cat))
        : bulkCategorizeTransactions(selectedIds, cat).then(() => undefined),
    onSuccess: () => {
      invalidate();
    },
  });

  if (count === 0) return null;

  return (
    <div className="flex items-center gap-3 flex-wrap px-4 py-3 border border-border-muted bg-highlight-muted/20">
      <span className="text-xs font-mono uppercase tracking-wider">
        {count} selected
      </span>

      <div className="h-5 w-px bg-border-muted" />

      <Button
        variant="secondary"
        disabled={approveMutation.isPending}
        onClick={() => approveMutation.mutate(true)}
      >
        Approve
      </Button>
      <Button
        variant="ghost"
        disabled={approveMutation.isPending}
        onClick={() => approveMutation.mutate(false)}
      >
        Unapprove
      </Button>

      <div className="h-5 w-px bg-border-muted" />

      <Select
        value={undefined}
        onValueChange={(val) => {
          if (categorizeMutation.isPending) return;
          categorizeMutation.mutate(
            val === undefined || val === '__clear__' ? null : val,
          );
        }}
        options={[
          { value: '__clear__', label: 'NONE' },
          ...categories.map((cat) => ({
            value: cat,
            label: cat.toUpperCase(),
          })),
        ]}
        placeholder={
          categorizeMutation.isPending ? 'Applying…' : 'Set category…'
        }
        className="h-8 py-0 px-3 text-xs font-mono uppercase tracking-wider w-44 bg-canvas border border-border-muted"
      />

      <div className="flex-1" />

      <button
        type="button"
        onClick={onClear}
        className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-100 underline underline-offset-4"
      >
        Clear selection
      </button>
    </div>
  );
};
