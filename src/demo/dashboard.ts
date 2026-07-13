import type {
  AccountSeries,
  AllocationPoint,
  CashFlowPoint,
  CategoryMeta,
  NetWorthPoint,
  SpendingPoint,
} from '@/hooks/dashboard/types';
import type { Account } from '@/types';

const MONTHS = [
  '2025-08',
  '2025-09',
  '2025-10',
  '2025-11',
  '2025-12',
  '2026-01',
  '2026-02',
  '2026-03',
  '2026-04',
  '2026-05',
];

const makeHistory = (values: number[]) =>
  MONTHS.map((month, i) => ({ month, value: values[i] }));

export const demoAccounts: Account[] = [
  {
    id: 'a1',
    name: 'SEB Lönekonto',
    currency: 'SEK',
    account_type: 'checking',
    account_source: 'import',
    color: '#3b82f6',
    space_id: 'demo',
  },
  {
    id: 'a2',
    name: 'SEB Sparkonto',
    currency: 'SEK',
    account_type: 'savings',
    account_source: 'import',
    color: '#10b981',
    space_id: 'demo',
  },
  {
    id: 'a3',
    name: 'Avanza ISK',
    currency: 'SEK',
    account_type: 'investment',
    account_source: 'import',
    color: '#8b5cf6',
    space_id: 'demo',
  },
];

export const demoNetWorthSeries: NetWorthPoint[] = [
  { month: '2025-08', value: 420000 },
  { month: '2025-09', value: 438000 },
  { month: '2025-10', value: 451000 },
  { month: '2025-11', value: 445000 },
  { month: '2025-12', value: 463000 },
  { month: '2026-01', value: 478000 },
  { month: '2026-02', value: 492000 },
  { month: '2026-03', value: 487000 },
  { month: '2026-04', value: 511000 },
  { month: '2026-05', value: 534000 },
];

export const demoCashFlowSeries: CashFlowPoint[] = [
  { month: '2025-08', net: 12400 },
  { month: '2025-09', net: 18000 },
  { month: '2025-10', net: 13200 },
  { month: '2025-11', net: -4100 },
  { month: '2025-12', net: -8800 },
  { month: '2026-01', net: 15000 },
  { month: '2026-02', net: 14200 },
  { month: '2026-03', net: -5300 },
  { month: '2026-04', net: 24000 },
  { month: '2026-05', net: 23100 },
];

export const demoAccountSeries: AccountSeries[] = [
  {
    account: demoAccounts[0],
    latestBalance: 82340,
    history: makeHistory([
      61000, 65000, 70000, 68000, 72000, 75000, 78000, 76000, 80000, 82340,
    ]),
  },
  {
    account: demoAccounts[1],
    latestBalance: 150000,
    history: makeHistory([
      120000, 122000, 125000, 127000, 130000, 133000, 138000, 141000, 146000,
      150000,
    ]),
  },
  {
    account: demoAccounts[2],
    latestBalance: 301660,
    history: makeHistory([
      239000, 251000, 256000, 250000, 261000, 270000, 276000, 270000, 285000,
      301660,
    ]),
  },
];

export const demoAllocationSeries: AllocationPoint[] = MONTHS.map(
  (month, i) => ({
    month,
    a1: demoAccountSeries[0].history[i].value,
    a2: demoAccountSeries[1].history[i].value,
    a3: demoAccountSeries[2].history[i].value,
  }),
);

export const demoSpendingCategories: CategoryMeta[] = [
  { name: 'Groceries', color: 'oklch(58% 0.19 145)' },
  { name: 'Transport', color: 'oklch(50% 0.25 264)' },
  { name: 'Dining', color: 'oklch(70% 0.19 75)' },
  { name: 'Subscriptions', color: 'oklch(52% 0.23 22)' },
  { name: 'Shopping', color: 'oklch(55% 0.18 310)' },
];

export const demoSpendingSeries: SpendingPoint[] = MONTHS.map((month) => ({
  month,
  Groceries: 3200 + Math.trunc(Math.random() * 800),
  Transport: 990,
  Dining: 1200 + Math.trunc(Math.random() * 600),
  Subscriptions: 400,
  Shopping: 800 + Math.trunc(Math.random() * 1200),
}));
