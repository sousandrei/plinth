import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';

import { listAccounts } from '@/api/accounts';
import { AccountsTable } from '@/components/accounts/AccountsTable';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { useAuth } from '@/context/AuthContext';

function Accounts(): React.JSX.Element {
  const { user } = useAuth();

  const {
    data: accounts,
    isLoading,
    isError,
  } = useQuery({
    queryKey: ['accounts', user?.id],
    queryFn: () => listAccounts(user?.id ?? ''),
    enabled: !!user,
  });

  return (
    <div className="max-w-[1200px] mx-auto px-6 py-10 flex flex-col gap-8">
      <div className="flex flex-col gap-1 animate-fade-in">
        <h1 className="text-2xl font-semibold tracking-tight">Accounts</h1>
        <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
          {isLoading ? 'Loading…' : `${accounts?.length ?? 0} accounts`}
        </p>
      </div>

      <div className="animate-fade-in">
        <Card>
          <CardHeader
            label="All accounts"
            meta={accounts ? String(accounts.length) : undefined}
          />
          <CardBody className="p-0">
            {isLoading && (
              <div className="flex flex-col divide-y divide-border-subtle">
                {Array.from({ length: 4 }).map((_, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: skeleton placeholder
                  <div key={i} className="px-6 py-3 flex gap-6">
                    <div className="h-4 w-48 bg-muted animate-pulse" />
                    <div className="h-4 w-20 bg-muted animate-pulse" />
                    <div className="h-4 w-16 bg-muted animate-pulse" />
                  </div>
                ))}
              </div>
            )}

            {isError && (
              <p className="px-6 py-8 text-xs font-mono text-expense text-center">
                Failed to load accounts.
              </p>
            )}

            {accounts && accounts.length === 0 && (
              <p className="px-6 py-8 text-xs font-mono text-muted-foreground text-center">
                No accounts yet. Import a file to get started.
              </p>
            )}

            {accounts && accounts.length > 0 && (
              <AccountsTable accounts={accounts} />
            )}
          </CardBody>
        </Card>
      </div>
    </div>
  );
}

export const Route = createFileRoute('/accounts')({
  component: Accounts,
});
