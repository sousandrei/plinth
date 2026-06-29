import type { CashFlowPoint, NetWorthPoint } from './types';

// Cash flow per month = month-over-month change in total balance across
// all accounts. Internal transfers (between own accounts) cancel out
// automatically because total balance is unchanged; no transfer marker
// needed. The net worth series already carries forward each account's
// last-known balance, so we just diff it. The first month in the series
// has no prior value and is reported as 0. Only the most recent 12
// months are returned.
export function deriveCashFlow(
  netWorthSeries: NetWorthPoint[],
): CashFlowPoint[] {
  const deltas: CashFlowPoint[] = netWorthSeries.map((point, i) => ({
    month: point.month,
    net: i === 0 ? 0 : point.value - netWorthSeries[i - 1].value,
  }));
  return deltas.slice(-12);
}
