import type { Account, Aggregations } from '@/types';
import { type AccountSeries, toMajor } from './types';

export function deriveAccounts(
  aggregations: Aggregations,
  months: string[],
  accounts: Account[],
): AccountSeries[] {
  return accounts.map((account) => {
    // Build raw monthly balance, then carry the last known value forward
    // so sparklines don't drop to zero for months with no transactions.
    const history = months
      .map((month) => ({
        month,
        value: toMajor(aggregations[month].balance[account.id] ?? 0),
      }))
      .reduce<{ month: string; value: number }[]>((acc, point) => {
        const prev = acc[acc.length - 1];
        const value = point.value === 0 && prev ? prev.value : point.value;
        acc.push({ month: point.month, value });
        return acc;
      }, []);

    const latestBalance =
      history.length > 0 ? history[history.length - 1].value : 0;

    return { account, latestBalance, history };
  });
}
