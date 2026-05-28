import { createFileRoute } from '@tanstack/react-router';
import { AccountsTable } from '@/components/dashboard/AccountsTable';
import { CashFlowChart } from '@/components/dashboard/CashFlowChart';
import { NetWorthAllocation } from '@/components/dashboard/NetWorthAllocation';
import { NetWorthHero } from '@/components/dashboard/NetWorthHero';
import { SpendingByCategory } from '@/components/dashboard/SpendingByCategory';
import { useAuth } from '@/context/AuthContext';
import { useDashboard } from '@/hooks/useDashboard';

export const Route = createFileRoute('/')({
  component: Dashboard,
});

function Dashboard(): React.JSX.Element {
  const { user } = useAuth();
  const { data } = useDashboard(user?.id ?? null);

  return (
    <div className="max-w-[1200px] mx-auto px-6 py-10 space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>

      {data && (
        <div className="space-y-4">
          {/* Row 1 — Net Worth / Cash Flow (50/50) */}
          <div className="grid grid-cols-2 gap-4">
            <div className="h-56">
              <NetWorthHero
                series={data.netWorthSeries}
                latestNetWorth={data.latestNetWorth}
                currency="SEK"
              />
            </div>
            <div className="h-56">
              <CashFlowChart series={data.cashFlowSeries} currency="SEK" />
            </div>
          </div>

          {/* Row 2 — Accounts / Allocation (60/40) */}
          <div className="grid grid-cols-5 gap-4">
            <div className="col-span-3">
              <AccountsTable
                accountSeries={data.accountSeries}
                currency="SEK"
              />
            </div>
            <div className="col-span-2">
              <NetWorthAllocation
                series={data.allocationSeries}
                accounts={data.accounts}
                currency="SEK"
              />
            </div>
          </div>

          {/* Row 3 — Spending by Category (full width) */}
          <div className="h-72">
            <SpendingByCategory
              series={data.spendingSeries}
              categories={data.categories}
              currency="SEK"
            />
          </div>
        </div>
      )}
    </div>
  );
}
