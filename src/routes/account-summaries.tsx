import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import {
  listAccountSummaries,
  upsertAccountSummary,
} from '@/api/accountSummaries';
import { listAccounts } from '@/api/accounts';
import { AccountSummaryRow } from '@/components/account-summaries/AccountSummaryRow';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import { Pagination } from '@/components/ui/Pagination';
import { Select } from '@/components/ui/Select';
import { useAuth } from '@/context/AuthContext';
import { cn } from '@/lib/util';

const PAGE_SIZE = 15;

export const Route = createFileRoute('/account-summaries')({
  component: AccountSummaries,
});

function AccountSummaries(): React.JSX.Element {
  const { user } = useAuth();
  const queryClient = useQueryClient();
  const [page, setPage] = useState(0);

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll to top on page change
  useEffect(() => {
    window.scrollTo({ top: 0, behavior: 'instant' });
  }, [page]);

  const { data, isLoading, isError, isFetching } = useQuery({
    queryKey: ['account-summaries', user?.id, page],
    queryFn: () => listAccountSummaries(user?.id ?? '', page, PAGE_SIZE),
    enabled: !!user,
    placeholderData: (prev) => prev,
  });

  const summaries = data?.rows ?? [];
  const pageCount = data?.page_count ?? 1;

  const { data: accounts = [] } = useQuery({
    queryKey: ['accounts', user?.id],
    queryFn: () => listAccounts(user?.id ?? ''),
    enabled: !!user,
  });

  const accountMap = Object.fromEntries(accounts.map((a) => [a.id, a]));

  // Add new summary dialog state
  const [addOpen, setAddOpen] = useState(false);
  const [addMonth, setAddMonth] = useState('');
  const [addAccountId, setAddAccountId] = useState<string | null>(null);
  const [addBalance, setAddBalance] = useState('');

  const addMutation = useMutation({
    mutationFn: () => {
      if (!addAccountId)
        return Promise.reject(new Error('No account selected'));
      return upsertAccountSummary(
        addMonth,
        addAccountId,
        Math.round(parseFloat(addBalance) * 100),
      );
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['account-summaries'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
      setPage(0);
      setAddOpen(false);
      setAddMonth('');
      setAddAccountId(null);
      setAddBalance('');
    },
    onError: (err: unknown) => {
      alert(`Failed: ${err instanceof Error ? err.message : String(err)}`);
    },
  });

  const canAdd =
    !addMutation.isPending &&
    addMonth.match(/^\d{4}-\d{2}$/) !== null &&
    addAccountId !== null &&
    addBalance.trim() !== '' &&
    !Number.isNaN(parseFloat(addBalance));

  const accountOptions = accounts.map((a) => ({ value: a.id, label: a.name }));

  return (
    <div className="max-w-[1200px] mx-auto px-6 py-10 flex flex-col gap-6">
      <div className="flex items-center justify-between animate-fade-in">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            Account Summaries
          </h1>
          <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
            {isLoading ? 'Loading…' : `${pageCount * PAGE_SIZE}+ entries`}{' '}
          </p>
        </div>

        <Dialog open={addOpen} onOpenChange={setAddOpen}>
          <DialogTrigger
            render={<Button variant="primary">Add Entry</Button>}
          />
          <DialogContent
            title="Add Balance Entry"
            description="Manually record a monthly balance for an investment account."
          >
            <div className="space-y-5">
              <div className="space-y-1.5 text-left">
                <label
                  htmlFor="add-month"
                  className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
                >
                  Month (YYYY-MM)
                </label>
                <Input
                  id="add-month"
                  placeholder="2025-01"
                  value={addMonth}
                  onChange={(e) => setAddMonth(e.target.value)}
                />
              </div>
              <div className="space-y-1.5 text-left">
                <label
                  htmlFor="add-account"
                  className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
                >
                  Account
                </label>
                <Select
                  options={accountOptions}
                  value={addAccountId ?? undefined}
                  onValueChange={setAddAccountId}
                  placeholder="Select account…"
                />
              </div>
              <div className="space-y-1.5 text-left">
                <label
                  htmlFor="add-balance"
                  className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
                >
                  Balance (
                  {addAccountId
                    ? (accountMap[addAccountId]?.currency ?? 'SEK')
                    : 'SEK'}
                  )
                </label>
                <Input
                  id="add-balance"
                  type="number"
                  placeholder="150000"
                  value={addBalance}
                  onChange={(e) => setAddBalance(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && canAdd) addMutation.mutate();
                  }}
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
                  onClick={() => addMutation.mutate()}
                  disabled={!canAdd}
                  className="px-4 py-2 text-xs rounded-none h-9"
                >
                  {addMutation.isPending ? 'Saving…' : 'Add'}
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      </div>

      <div
        className={cn(
          'animate-fade-in transition-opacity duration-150',
          isFetching && 'opacity-60',
        )}
      >
        <Card>
          <CardHeader
            label="All entries"
            meta={
              isLoading || summaries.length === 0
                ? undefined
                : `Page ${page + 1} of ${pageCount}`
            }
          />
          <CardBody className="p-0">
            {isLoading && (
              <div className="flex flex-col divide-y divide-border-subtle">
                {Array.from({ length: 6 }).map((_, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: skeleton
                  <div key={i} className="px-6 py-3 flex gap-6">
                    <div className="h-4 w-32 bg-muted animate-pulse" />
                    <div className="h-4 w-40 bg-muted animate-pulse" />
                    <div className="h-4 w-24 bg-muted animate-pulse" />
                  </div>
                ))}
              </div>
            )}

            {isError && (
              <p className="px-6 py-8 text-xs font-mono text-expense text-center">
                Failed to load account summaries.
              </p>
            )}

            {!isLoading && !isError && summaries.length === 0 && (
              <p className="px-6 py-8 text-xs font-mono text-muted-foreground text-center">
                No entries yet.
              </p>
            )}

            {summaries.length > 0 && (
              <table className="w-full text-sm border-collapse">
                <thead>
                  <tr className="border-b border-foreground/10 bg-foreground">
                    <th className="px-6 py-3 text-left text-xs font-mono uppercase tracking-widest text-canvas/60">
                      Month
                    </th>
                    <th className="px-6 py-3 text-left text-xs font-mono uppercase tracking-widest text-canvas/60">
                      Account
                    </th>
                    <th className="px-6 py-3 text-left text-xs font-mono uppercase tracking-widest text-canvas/60">
                      Type
                    </th>
                    <th className="px-6 py-3 text-right text-xs font-mono uppercase tracking-widest text-canvas/60">
                      Balance
                    </th>
                    <th className="px-6 py-3 text-right text-xs font-mono uppercase tracking-widest text-canvas/60">
                      Actions
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {summaries.map((s) => (
                    <AccountSummaryRow
                      key={`${s.month}-${s.account_id}`}
                      summary={s}
                      account={accountMap[s.account_id]}
                    />
                  ))}
                </tbody>
              </table>
            )}
          </CardBody>
        </Card>
      </div>

      <Pagination page={page} pageCount={pageCount} onPageChange={setPage} />
    </div>
  );
}
