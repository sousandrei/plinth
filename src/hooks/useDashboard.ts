import { useQuery } from '@tanstack/react-query';
import { listAccounts } from '@/api/accounts';
import { getAggregations } from '@/api/aggregations';
import type { Account, Aggregations } from '@/types';
import { deriveAccounts } from './dashboard/deriveAccounts';
import { deriveAllocation } from './dashboard/deriveAllocation';
import { deriveCashFlow } from './dashboard/deriveCashFlow';
import { deriveNetWorth } from './dashboard/deriveNetWorth';
import { deriveSpending } from './dashboard/deriveSpending';
import type { DashboardData } from './dashboard/types';

export type {
  AccountSeries,
  AllocationPoint,
  CashFlowPoint,
  CategoryMeta,
  DashboardData,
  NetWorthPoint,
  SpendingPoint,
} from './dashboard/types';

function deriveData(
  aggregations: Aggregations,
  accounts: Account[],
): DashboardData {
  const months = Object.keys(aggregations).sort();

  const { series: netWorthSeries, latest: latestNetWorth } = deriveNetWorth(
    aggregations,
    months,
  );
  const cashFlowSeries = deriveCashFlow(aggregations, months);
  const accountSeries = deriveAccounts(aggregations, months, accounts);
  const { series: spendingSeries, categories } = deriveSpending(
    aggregations,
    months,
  );
  const allocationSeries = deriveAllocation(aggregations, months, accounts);

  return {
    months,
    netWorthSeries,
    latestNetWorth,
    cashFlowSeries,
    accountSeries,
    spendingSeries,
    categories,
    allocationSeries,
    accounts,
  };
}

export function useDashboard(userId: string | null): {
  data: DashboardData | null;
  isLoading: boolean;
  isError: boolean;
} {
  const accountsQuery = useQuery({
    queryKey: ['accounts', userId],
    queryFn: () => listAccounts(userId ?? ''),
    enabled: !!userId,
  });

  const aggregationsQuery = useQuery({
    queryKey: ['aggregations', userId],
    queryFn: () => getAggregations(userId ?? ''),
    enabled: !!userId,
  });

  const isLoading = accountsQuery.isLoading || aggregationsQuery.isLoading;
  const isError = accountsQuery.isError || aggregationsQuery.isError;

  const data =
    accountsQuery.data && aggregationsQuery.data
      ? deriveData(aggregationsQuery.data, accountsQuery.data)
      : null;

  return { data, isLoading, isError };
}
