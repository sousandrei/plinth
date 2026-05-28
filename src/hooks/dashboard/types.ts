import type { Account } from '@/types';

export const toMajor = (minor: number): number => minor / 100;

export interface NetWorthPoint {
  month: string;
  value: number;
}

export interface CashFlowPoint {
  month: string;
  net: number;
}

export interface AccountSeries {
  account: Account;
  latestBalance: number;
  history: { month: string; value: number }[];
}

export interface SpendingPoint {
  month: string;
  [category: string]: string | number;
}

export interface AllocationPoint {
  month: string;
  [accountId: string]: string | number;
}

export interface CategoryMeta {
  name: string;
  color: string;
}

export interface DashboardData {
  months: string[];
  netWorthSeries: NetWorthPoint[];
  latestNetWorth: number;
  cashFlowSeries: CashFlowPoint[];
  accountSeries: AccountSeries[];
  spendingSeries: SpendingPoint[];
  categories: CategoryMeta[];
  allocationSeries: AllocationPoint[];
  accounts: Account[];
}
