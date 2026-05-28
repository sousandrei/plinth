import type { Aggregations } from '@/types';
import { type CashFlowPoint, toMajor } from './types';

// Sum of all category amounts per month.
// Positive amounts are income, negative are expenses; net is their sum.
// Only the most recent 12 months are returned.
export function deriveCashFlow(
  aggregations: Aggregations,
  months: string[],
): CashFlowPoint[] {
  const window = months.slice(-12);
  return window.map((month) => {
    const net = Object.values(aggregations[month].by_category).reduce(
      (sum, v) => sum + v,
      0,
    );
    return { month, net: toMajor(net) };
  });
}
