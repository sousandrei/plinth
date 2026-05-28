import type { Aggregations } from '@/types';
import { type NetWorthPoint, toMajor } from './types';

export function deriveNetWorth(
  aggregations: Aggregations,
  months: string[],
): { series: NetWorthPoint[]; latest: number } {
  // Carry the last known balance forward for each account so that months
  // where an account has no transaction or summary entry still contribute
  // their most recent known value to the total net worth.
  const lastKnown: Record<string, number> = {};

  const series: NetWorthPoint[] = months.map((month) => {
    const balances = aggregations[month].balance;

    for (const [accountId, value] of Object.entries(balances)) {
      lastKnown[accountId] = value;
    }

    const total = Object.values(lastKnown).reduce((sum, v) => sum + v, 0);
    return { month, value: toMajor(total) };
  });

  const latest = series.length > 0 ? series[series.length - 1].value : 0;
  return { series, latest };
}
