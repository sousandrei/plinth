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
import { toast } from '@/components/ui/Toast';
import { cn } from '@/lib/util';
import type { Account } from '@/types';

const PRESET_COLORS = [
  '#22c55e', // Green
  '#10b981', // Emerald
  '#06b6d4', // Cyan
  '#3b82f6', // Blue
  '#6366f1', // Indigo
  '#f97316', // Orange
  '#ec4899', // Pink
  '#8b5cf6', // Purple
  '#eab308', // Yellow
  '#ef4444', // Red
  '#6b7280', // Gray
];

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
  const [selectedColor, setSelectedColor] = useState(account.color);

  // Reset draft & color when modal opens
  useEffect(() => {
    if (open) {
      setDraft(account.name);
      setSelectedColor(account.color);
    }
  }, [open, account.name, account.color]);

  const mutation = useMutation({
    mutationFn: ({ name, color }: { name: string; color: string }) =>
      updateAccount(account.id, name, color),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['accounts'] });
      setOpen(false);
    },
    onError: (err: unknown) => {
      toast.error(
        'Update failed',
        err instanceof Error ? err.message : String(err),
      );
      setDraft(account.name);
      setSelectedColor(account.color);
    },
  });

  const commit = () => {
    const trimmedName = draft.trim();
    if (!trimmedName) {
      setDraft(account.name);
      return;
    }
    if (trimmedName === account.name && selectedColor === account.color) {
      setOpen(false);
      return;
    }
    mutation.mutate({ name: trimmedName, color: selectedColor });
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (
      e.key === 'Enter' &&
      !mutation.isPending &&
      draft.trim() &&
      (draft.trim() !== account.name || selectedColor !== account.color)
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
      {/* Name with Color dot */}
      <td className="px-6 py-4 font-medium text-foreground">
        <div className="flex items-center gap-2.5">
          <span
            className="w-2.5 h-2.5 rounded-full shrink-0 border border-black/10 dark:border-white/10 shadow-sm"
            style={{ backgroundColor: account.color }}
          />
          {account.name}
        </div>
      </td>

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
            description="Update account display name, theme color, and view account details."
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

              {/* Theme Color Picker */}
              <div className="space-y-2 text-left">
                <span className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold">
                  Theme Color
                </span>
                <div className="flex flex-wrap gap-2 items-center">
                  {PRESET_COLORS.map((preset) => (
                    <button
                      key={preset}
                      type="button"
                      onClick={() => setSelectedColor(preset)}
                      className={cn(
                        'w-6 h-6 rounded-full cursor-pointer transition-all duration-150 border-2',
                        selectedColor === preset
                          ? 'border-foreground scale-110 shadow-sm'
                          : 'border-transparent hover:scale-105',
                      )}
                      style={{ backgroundColor: preset }}
                      aria-label={`Select color ${preset}`}
                    />
                  ))}
                  {/* Custom color input */}
                  <label
                    className="w-6 h-6 rounded-full cursor-pointer border border-border-subtle hover:scale-105 transition-transform flex items-center justify-center overflow-hidden relative"
                    style={{
                      backgroundColor: PRESET_COLORS.includes(selectedColor)
                        ? 'transparent'
                        : selectedColor,
                    }}
                  >
                    <span className="text-[10px] font-mono text-muted-foreground select-none">
                      🎨
                    </span>
                    <input
                      type="color"
                      value={selectedColor}
                      onChange={(e) => setSelectedColor(e.target.value)}
                      className="absolute inset-0 opacity-0 cursor-pointer w-full h-full"
                    />
                  </label>
                </div>
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
                    (draft.trim() === account.name &&
                      selectedColor === account.color)
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
