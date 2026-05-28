import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import {
  deleteAccountSummary,
  upsertAccountSummary,
} from '@/api/accountSummaries';
import { Button } from '@/components/ui/Button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import type { Account, AccountSummary } from '@/types';

interface Props {
  summary: AccountSummary;
  account: Account | undefined;
}

const fmt = (minor: number, currency: string): string =>
  new Intl.NumberFormat('sv-SE', {
    style: 'currency',
    currency,
    maximumFractionDigits: 0,
  }).format(minor / 100);

const fmtMonth = (ym: string): string => {
  const parts = ym.split('-');
  if (parts.length < 2) return ym;
  const year = Number(parts[0]);
  const month = Number(parts[1]);
  if (Number.isNaN(year) || Number.isNaN(month)) return ym;
  return new Date(year, month - 1).toLocaleString(undefined, {
    month: 'long',
    year: 'numeric',
  });
};

export const AccountSummaryRow = ({
  summary,
  account,
}: Props): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [editOpen, setEditOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  // Balance stored as major units in the input
  const [draft, setDraft] = useState(String(summary.balance / 100));

  useEffect(() => {
    if (editOpen) setDraft(String(summary.balance / 100));
  }, [editOpen, summary.balance]);

  const upsertMutation = useMutation({
    mutationFn: () =>
      upsertAccountSummary(
        summary.month,
        summary.account_id,
        Math.round(parseFloat(draft) * 100),
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['account-summaries'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
      setEditOpen(false);
    },
    onError: (err: unknown) => {
      alert(
        `Update failed: ${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => deleteAccountSummary(summary.month, summary.account_id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['account-summaries'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
      setDeleteOpen(false);
    },
    onError: (err: unknown) => {
      alert(
        `Delete failed: ${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  const canSave =
    !upsertMutation.isPending &&
    draft.trim() !== '' &&
    !Number.isNaN(parseFloat(draft)) &&
    Math.round(parseFloat(draft) * 100) !== summary.balance;

  return (
    <tr className="group border-b border-border-subtle last:border-0 hover:bg-muted/40 transition-colors duration-100">
      <td className="px-6 py-3 font-mono text-sm text-muted-foreground">
        {fmtMonth(summary.month)}
      </td>
      <td className="px-6 py-3 text-sm font-medium text-foreground">
        {account?.name ?? summary.account_id}
      </td>
      <td className="px-6 py-3 text-xs font-mono uppercase tracking-wider text-muted-foreground">
        {account?.account_type ?? '—'}
      </td>
      <td className="px-6 py-3 text-right font-mono text-sm font-semibold tabular-nums text-foreground">
        {fmt(summary.balance, account?.currency ?? 'SEK')}
      </td>
      <td className="px-6 py-3 text-right">
        <div className="flex items-center justify-end gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
          {/* Edit */}
          <Dialog open={editOpen} onOpenChange={setEditOpen}>
            <DialogTrigger
              render={
                <Button
                  variant="secondary"
                  className="px-3 py-1 text-xs font-mono uppercase tracking-wider h-7"
                >
                  Edit
                </Button>
              }
            />
            <DialogContent
              title="Edit Balance"
              description={`Update the balance for ${account?.name ?? summary.account_id} — ${fmtMonth(summary.month)}.`}
            >
              <div className="space-y-5">
                <div className="space-y-1.5 text-left">
                  <label
                    htmlFor="summary-balance"
                    className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
                  >
                    Balance ({account?.currency ?? 'SEK'})
                  </label>
                  <Input
                    id="summary-balance"
                    type="number"
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && canSave) upsertMutation.mutate();
                    }}
                    disabled={upsertMutation.isPending}
                  />
                </div>
                <div className="flex justify-end gap-2 pt-3 border-t border-border-subtle">
                  <DialogClose
                    render={
                      <Button
                        variant="secondary"
                        className="px-4 py-2 text-xs rounded-none h-9"
                      >
                        Cancel
                      </Button>
                    }
                  />
                  <Button
                    variant="primary"
                    onClick={() => upsertMutation.mutate()}
                    disabled={!canSave}
                    className="px-4 py-2 text-xs rounded-none h-9"
                  >
                    {upsertMutation.isPending ? 'Saving…' : 'Save'}
                  </Button>
                </div>
              </div>
            </DialogContent>
          </Dialog>

          {/* Delete */}
          <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
            <DialogTrigger
              render={
                <Button
                  variant="ghost"
                  className="px-3 py-1 text-xs font-mono uppercase tracking-wider h-7 text-expense hover:text-expense"
                >
                  Delete
                </Button>
              }
            />
            <DialogContent
              title="Delete Summary"
              description={`Remove the balance entry for ${account?.name ?? summary.account_id} — ${fmtMonth(summary.month)}? This cannot be undone.`}
            >
              <div className="flex justify-end gap-2 pt-3 mt-4 border-t border-border-subtle">
                <DialogClose
                  render={
                    <Button
                      variant="secondary"
                      className="px-4 py-2 text-xs rounded-none h-9"
                    >
                      Cancel
                    </Button>
                  }
                />
                <Button
                  variant="primary"
                  onClick={() => deleteMutation.mutate()}
                  disabled={deleteMutation.isPending}
                  className="px-4 py-2 text-xs rounded-none h-9 bg-expense border-expense"
                >
                  {deleteMutation.isPending ? 'Deleting…' : 'Delete'}
                </Button>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </td>
    </tr>
  );
};
