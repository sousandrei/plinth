import type { Account, Aggregations } from '@/types';
import { type AllocationPoint, toMajor } from './types';

export function deriveAllocation(
  aggregations: Aggregations,
  months: string[],
  accounts: Account[],
): AllocationPoint[] {
  // Track last known balance per account so sparse months (e.g. investment
  // accounts that only report a summary once a month) carry forward correctly.
  const lastKnown: Record<string, number> = {};

  return months.map((month) => {
    const balances = aggregations[month].balance;
    const point: AllocationPoint = { month };
    for (const account of accounts) {
      const raw = balances[account.id];
      if (raw !== undefined) {
        lastKnown[account.id] = toMajor(raw);
      }
      point[account.id] = lastKnown[account.id] ?? 0;
    }
    return point;
  });
}
