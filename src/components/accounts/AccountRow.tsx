import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { updateAccount } from '@/api/accounts';
import { Button } from '@/components/ui/Button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import { cn } from '@/lib/util';
import type { Account } from '@/types';

const TYPE_LABELS: Record<string, string> = {
  checking: 'Checking',
  savings: 'Savings',
  investment: 'Investment',
  credit: 'Credit',
  loan: 'Loan',
  other: 'Other',
};

const SOURCE_LABELS: Record<string, string> = {
  manual: 'Manual',
  import: 'Import',
  sync: 'Sync',
};

interface AccountRowProps {
  account: Account;
}

export const AccountRow = ({ account }: AccountRowProps): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(account.name);

  // Reset draft to current account name when modal opens
  useEffect(() => {
    if (open) {
      setDraft(account.name);
    }
  }, [open, account.name]);

  const mutation = useMutation({
    mutationFn: (name: string) => updateAccount(account.id, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['accounts'] });
      setOpen(false);
    },
    onError: (err: unknown) => {
      alert(
        `Update failed: ${err instanceof Error ? err.message : String(err)}`,
      );
      setDraft(account.name);
    },
  });

  const commit = () => {
    const trimmed = draft.trim();
    if (!trimmed) {
      setDraft(account.name);
      return;
    }
    if (trimmed === account.name) {
      setOpen(false);
      return;
    }
    mutation.mutate(trimmed);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (
      e.key === 'Enter' &&
      !mutation.isPending &&
      draft.trim() &&
      draft.trim() !== account.name
    ) {
      commit();
    }
  };

  return (
    <tr
      className={cn(
        'group border-b border-border-subtle last:border-0',
        'transition-colors duration-100',
        'hover:bg-muted/40',
      )}
    >
      {/* Name */}
      <td className="px-6 py-4 font-medium text-foreground">{account.name}</td>

      {/* Type */}
      <td className="px-6 py-4 whitespace-nowrap">
        <span className="text-xs font-mono text-muted-foreground uppercase tracking-wider">
          {TYPE_LABELS[account.account_type] ?? account.account_type}
        </span>
      </td>

      {/* Source */}
      <td className="px-6 py-4 whitespace-nowrap">
        <span className="text-xs font-mono text-muted-foreground uppercase tracking-wider">
          {SOURCE_LABELS[account.account_source] ?? account.account_source}
        </span>
      </td>

      {/* Currency */}
      <td className="px-6 py-4 whitespace-nowrap text-right">
        <span className="text-xs font-mono font-semibold uppercase">
          {account.currency}
        </span>
      </td>

      {/* Actions */}
      <td className="px-6 py-4 whitespace-nowrap text-right">
        <Dialog open={open} onOpenChange={setOpen}>
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
            title="Edit Account"
            description="Update account display name and view account details."
          >
            <div className="space-y-5">
              {/* Readonly info grid */}
              <div className="grid grid-cols-2 gap-4 p-4 bg-canvas border border-border-subtle rounded-none text-xs font-mono text-left">
                <div>
                  <span className="text-muted-foreground block uppercase tracking-wider text-[10px] mb-0.5">
                    ID
                  </span>
                  <span className="text-foreground select-all break-all">
                    {account.id}
                  </span>
                </div>
                <div>
                  <span className="text-muted-foreground block uppercase tracking-wider text-[10px] mb-0.5">
                    Currency
                  </span>
                  <span className="text-foreground uppercase font-semibold">
                    {account.currency}
                  </span>
                </div>
                <div>
                  <span className="text-muted-foreground block uppercase tracking-wider text-[10px] mb-0.5">
                    Type
                  </span>
                  <span className="text-foreground">
                    {TYPE_LABELS[account.account_type] ?? account.account_type}
                  </span>
                </div>
                <div>
                  <span className="text-muted-foreground block uppercase tracking-wider text-[10px] mb-0.5">
                    Source
                  </span>
                  <span className="text-foreground">
                    {SOURCE_LABELS[account.account_source] ??
                      account.account_source}
                  </span>
                </div>
              </div>

              {/* Editable Name Field */}
              <div className="space-y-1.5 text-left">
                <label
                  htmlFor="account-name"
                  className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
                >
                  Account Name
                </label>
                <Input
                  id="account-name"
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder="Enter account name..."
                  disabled={mutation.isPending}
                />
              </div>

              {/* Actions Footer */}
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
                  onClick={commit}
                  disabled={
                    mutation.isPending ||
                    !draft.trim() ||
                    draft.trim() === account.name
                  }
                  className="px-4 py-2 text-xs rounded-none h-9"
                >
                  {mutation.isPending ? 'Saving...' : 'Save Changes'}
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      </td>
    </tr>
  );
};
