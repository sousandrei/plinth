import { useMutation, useQueryClient } from '@tanstack/react-query';
import { createPortal } from 'react-dom';

import {
  bulkApproveTransactions,
  bulkCategorizeTransactions,
} from '@/api/transactions';
import { classifyTransactions } from '@/api/upload';
import { Button } from '@/components/ui/Button';
import { Select } from '@/components/ui/Select';
import { toast } from '@/components/ui/Toast';
import type { TransactionPage } from '@/types';

interface BulkActionBarProps {
  selectedIds: string[];
  categories: string[];
  isDemoMode: boolean;
  onClear: () => void;
}

const describePredictError = (err: unknown): string => {
  const msg = err instanceof Error ? err.message : String(err);
  if (msg.includes('classifier not ready')) {
    return 'Train a model first — the classifier has nothing loaded yet.';
  }
  return msg;
};

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

  const predictMutation = useMutation({
    mutationFn: async () => {
      if (isDemoMode) {
        // Look up selected rows from the demo cache so we can show
        // predictions in-place without touching the DB. The transactions
        // query is keyed by [transactions, demo, page, ...filters] so
        // we use a prefix match to gather every cached page.
        const pages = queryClient.getQueriesData<TransactionPage>({
          queryKey: ['transactions', 'demo'],
        });
        const selected: Array<{
          id: string;
          text: string;
          amount: number;
          booking_date: string;
        }> = [];
        for (const [, data] of pages) {
          if (!data) continue;
          for (const t of data.transactions) {
            if (selectedIds.includes(t.id)) {
              selected.push({
                id: t.id,
                text: t.text,
                amount: t.amount,
                booking_date: t.booking_date,
              });
            }
          }
        }
        if (selected.length === 0) {
          throw new Error('No selected transactions found in cache.');
        }
        const predictions = await classifyTransactions(selected);
        queryClient.setQueriesData<TransactionPage>(
          { queryKey: ['transactions', 'demo'] },
          (old) => {
            if (!old) return old;
            const byId = new Map(
              selected.map((s, i) => [s.id, predictions[i] ?? '']),
            );
            return {
              ...old,
              transactions: old.transactions.map((t) =>
                byId.has(t.id) ? { ...t, category: byId.get(t.id) ?? '' } : t,
              ),
            };
          },
        );
        return;
      }
      // Real flow: gather the selected transactions' inputs across every
      // cached page, run the classifier once, then apply each prediction
      // back to the DB.
      const pages = queryClient.getQueriesData<TransactionPage>({
        queryKey: ['transactions', 'live'],
      });
      const selectedInputs: Array<{
        text: string;
        amount: number;
        booking_date: string;
      }> = [];
      const idOrder: string[] = [];
      for (const [, data] of pages) {
        if (!data) continue;
        for (const t of data.transactions) {
          if (selectedIds.includes(t.id)) {
            selectedInputs.push({
              text: t.text,
              amount: t.amount,
              booking_date: t.booking_date,
            });
            idOrder.push(t.id);
          }
        }
      }
      if (selectedInputs.length === 0) {
        throw new Error('No selected transactions found in cache.');
      }
      const predictions = await classifyTransactions(selectedInputs);
      // Apply each prediction to the matching row. The classifier can
      // return an empty string when no model is loaded — skip those.
      for (let i = 0; i < idOrder.length; i++) {
        const cat = predictions[i];
        if (cat && cat.length > 0) {
          await bulkCategorizeTransactions([idOrder[i]], cat);
        }
      }
    },
    onSuccess: () => {
      invalidate();
    },
    onError: (err: unknown) => {
      toast.error('Prediction failed', describePredictError(err));
    },
  });

  if (count === 0) return null;

  return createPortal(
    <div className="fixed bottom-6 inset-x-0 flex justify-center z-50 pointer-events-none">
      <div className="pointer-events-auto animate-slide-up">
        <div className="flex items-center gap-3 px-4 py-3 bg-canvas border border-border-muted shadow-[0_8px_32px_-4px_oklch(0%_0_0_/_0.25)] backdrop-blur-sm">
          <span className="text-xs font-mono uppercase tracking-wider whitespace-nowrap">
            {count} selected
          </span>

          <div className="h-5 w-px bg-border-muted" />

          <Button
            variant="secondary"
            disabled={approveMutation.isPending || predictMutation.isPending}
            onClick={() => approveMutation.mutate(true)}
            className="px-3 py-1.5 text-xs h-8"
          >
            Approve
          </Button>
          <Button
            variant="ghost"
            disabled={approveMutation.isPending}
            onClick={() => approveMutation.mutate(false)}
            className="px-3 py-1.5 text-xs h-8"
          >
            Unapprove
          </Button>

          <div className="h-5 w-px bg-border-muted" />

          <Button
            variant="secondary"
            disabled={predictMutation.isPending}
            onClick={() => predictMutation.mutate()}
            className="px-3 py-1.5 text-xs h-8"
          >
            {predictMutation.isPending ? 'Predicting…' : 'Predict'}
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

          <div className="h-5 w-px bg-border-muted" />

          <button
            type="button"
            onClick={onClear}
            className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-100 underline underline-offset-4 whitespace-nowrap"
          >
            Clear
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
};
