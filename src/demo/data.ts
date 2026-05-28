/**
 * Deterministic demo data generator.
 * Uses a seeded LCG so the output is identical on every call / hot-reload.
 * Generates 5 years of realistic Swedish personal-finance transactions.
 */

import type { Account, Category, ModelCard, Transaction } from '@/types';

// ---------------------------------------------------------------------------
// Seeded PRNG — LCG (Knuth)
// ---------------------------------------------------------------------------

class Rng {
  private s: number;
  constructor(seed = 42) {
    this.s = seed >>> 0;
  }
  next(): number {
    this.s = Math.imul(this.s, 1664525) + 1013904223;
    return (this.s >>> 0) / 4294967295;
  }
  int(min: number, max: number): number {
    return Math.floor(this.next() * (max - min + 1)) + min;
  }
  pick<T>(arr: readonly T[]): T {
    return arr[this.int(0, arr.length - 1)];
  }
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

export const DEMO_ACCOUNTS: Account[] = [
  {
    id: 'demo-checking',
    name: 'Demo Checking',
    currency: 'SEK',
    account_type: 'checking',
    account_source: 'import',
    color: '#3b82f6',
    user_id: 'demo',
  },
  {
    id: 'demo-savings',
    name: 'Demo Savings',
    currency: 'SEK',
    account_type: 'savings',
    account_source: 'import',
    color: '#10b981',
    user_id: 'demo',
  },
  {
    id: 'demo-invest',
    name: 'Demo ISK',
    currency: 'SEK',
    account_type: 'investment',
    account_source: 'import',
    color: '#8b5cf6',
    user_id: 'demo',
  },
];

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

export const DEMO_CATEGORIES: Category[] = [
  { id: 'c1', name: 'Food & Drinks', color: '#f59e0b' },
  { id: 'c2', name: 'Transport', color: '#3b82f6' },
  { id: 'c3', name: 'Shopping', color: '#8b5cf6' },
  { id: 'c4', name: 'Leisure', color: '#ec4899' },
  { id: 'c5', name: 'Health & Beauty', color: '#10b981' },
  { id: 'c6', name: 'Household & Services', color: '#64748b' },
  { id: 'c7', name: 'Salary', color: '#22c55e' },
  { id: 'c8', name: 'Other Income', color: '#a3e635' },
  { id: 'c9', name: 'Savings & Investments', color: '#06b6d4' },
  { id: 'c10', name: 'Other', color: '#6b7280' },
];

// ---------------------------------------------------------------------------
// Merchant templates — (name, category, account, amount range in öre)
// ---------------------------------------------------------------------------

type Merchant = {
  name: string;
  cat: string;
  acct: 'demo-checking' | 'demo-savings' | 'demo-invest';
  min: number; // öre
  max: number;
};

const MERCHANTS: Merchant[] = [
  // Food
  {
    name: 'ICA Maxi',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 25000,
    max: 80000,
  },
  {
    name: 'Willys',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 20000,
    max: 55000,
  },
  {
    name: 'Coop',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 18000,
    max: 65000,
  },
  {
    name: 'Lidl',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 15000,
    max: 40000,
  },
  {
    name: 'Restaurang Pelikan',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 35000,
    max: 140000,
  },
  {
    name: 'Restaurang AG',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 55000,
    max: 180000,
  },
  {
    name: 'Espresso House',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 4500,
    max: 12000,
  },
  {
    name: "Wayne's Coffee",
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 3800,
    max: 9500,
  },
  {
    name: 'Systembolaget',
    cat: 'Food & Drinks',
    acct: 'demo-checking',
    min: 15000,
    max: 40000,
  },
  // Transport
  {
    name: 'SL Månadskort',
    cat: 'Transport',
    acct: 'demo-checking',
    min: 99500,
    max: 99500,
  },
  {
    name: 'Taxi Stockholm',
    cat: 'Transport',
    acct: 'demo-checking',
    min: 18000,
    max: 55000,
  },
  {
    name: 'Bolt',
    cat: 'Transport',
    acct: 'demo-checking',
    min: 8000,
    max: 25000,
  },
  {
    name: 'Biltvätt',
    cat: 'Transport',
    acct: 'demo-checking',
    min: 9900,
    max: 24900,
  },
  // Shopping
  {
    name: 'H&M',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 29900,
    max: 120000,
  },
  {
    name: 'Zalando',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 39900,
    max: 200000,
  },
  {
    name: 'IKEA',
    cat: 'Household & Services',
    acct: 'demo-checking',
    min: 49900,
    max: 350000,
  },
  {
    name: 'Stadium',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 49900,
    max: 250000,
  },
  {
    name: 'Intersport',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 49900,
    max: 280000,
  },
  {
    name: 'Åhlens',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 29900,
    max: 180000,
  },
  {
    name: 'Amazon',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 19900,
    max: 120000,
  },
  {
    name: 'Nelly.com',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 29900,
    max: 99900,
  },
  // Leisure
  {
    name: 'Netflix',
    cat: 'Leisure',
    acct: 'demo-checking',
    min: 13900,
    max: 13900,
  },
  {
    name: 'Spotify',
    cat: 'Leisure',
    acct: 'demo-checking',
    min: 10900,
    max: 10900,
  },
  {
    name: 'SF Bio',
    cat: 'Leisure',
    acct: 'demo-checking',
    min: 18000,
    max: 36000,
  },
  {
    name: 'Göteborgsoperan',
    cat: 'Leisure',
    acct: 'demo-checking',
    min: 25000,
    max: 90000,
  },
  {
    name: 'Elgiganten',
    cat: 'Shopping',
    acct: 'demo-checking',
    min: 49900,
    max: 500000,
  },
  // Health
  {
    name: 'Apoteket',
    cat: 'Health & Beauty',
    acct: 'demo-checking',
    min: 8900,
    max: 45000,
  },
  {
    name: 'Gymkort',
    cat: 'Health & Beauty',
    acct: 'demo-checking',
    min: 39900,
    max: 69900,
  },
  {
    name: 'Tandläkaren',
    cat: 'Health & Beauty',
    acct: 'demo-checking',
    min: 49500,
    max: 280000,
  },
  {
    name: 'Vårdcentral',
    cat: 'Health & Beauty',
    acct: 'demo-checking',
    min: 20000,
    max: 20000,
  },
  // Household
  {
    name: 'Vattenfall',
    cat: 'Household & Services',
    acct: 'demo-checking',
    min: 55000,
    max: 180000,
  },
  {
    name: 'Telia',
    cat: 'Household & Services',
    acct: 'demo-checking',
    min: 39900,
    max: 79900,
  },
  {
    name: 'Hemförsäkring',
    cat: 'Household & Services',
    acct: 'demo-checking',
    min: 180000,
    max: 280000,
  },
];

// Probability weights — merchants appearing more often get higher weight
const WEIGHTS = [
  10,
  10,
  10,
  8, // groceries
  4,
  3,
  6,
  5,
  3, // restaurants + café
  4,
  3,
  3,
  2, // transport
  4,
  4,
  2,
  2,
  2,
  3,
  3,
  3, // shopping
  2,
  2,
  3,
  1,
  2, // leisure
  4,
  3,
  2,
  2, // health
  3,
  2,
  1, // household
];

const WEIGHT_SUM = WEIGHTS.reduce((a, b) => a + b, 0);

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

type AggMonth = {
  by_category: Record<string, number>;
  balance: Record<string, number>;
};

export interface GeneratedData {
  transactions: Transaction[];
  aggregations: Record<string, AggMonth>;
  models: ModelCard[];
}

function pad(n: number, len = 2): string {
  return String(n).padStart(len, '0');
}

function dateStr(y: number, m: number, d: number): string {
  return `${y}-${pad(m)}-${pad(d)}`;
}

function pickWeighted(rng: Rng): Merchant {
  let r = rng.next() * WEIGHT_SUM;
  for (let i = 0; i < MERCHANTS.length; i++) {
    r -= WEIGHTS[i];
    if (r <= 0) return MERCHANTS[i];
  }
  return MERCHANTS[MERCHANTS.length - 1];
}

let _cache: GeneratedData | null = null;

export function getGeneratedData(): GeneratedData {
  if (_cache) return _cache;

  const rng = new Rng(42);
  const transactions: Transaction[] = [];

  // Running balances (öre)
  const bal: Record<string, number> = {
    'demo-checking': 5000000,
    'demo-savings': 1000000,
    'demo-invest': 1500000,
  };

  // Monthly accumulators for aggregations
  const aggMap: Record<string, AggMonth> = {};

  const today = new Date();
  const start = new Date(today);
  start.setFullYear(today.getFullYear() - 5);

  let txIdx = 0;

  // Iterate month by month
  let curYear = start.getFullYear();
  let curMonth = start.getMonth() + 1; // 1-indexed

  while (
    curYear < today.getFullYear() ||
    (curYear === today.getFullYear() && curMonth <= today.getMonth() + 1)
  ) {
    const monthKey = `${curYear}-${pad(curMonth)}`;
    const daysInMonth = new Date(curYear, curMonth, 0).getDate();
    const isCurrentMonth =
      curYear === today.getFullYear() && curMonth === today.getMonth() + 1;
    const lastDay = isCurrentMonth ? today.getDate() : daysInMonth;

    const agg: AggMonth = { by_category: {}, balance: {} };

    const addAmt = (
      acct: string,
      cat: string,
      amt: number,
      day: number,
      text: string,
      approved = true,
    ) => {
      const date = dateStr(curYear, curMonth, day);
      bal[acct] += amt;
      agg.by_category[cat] = (agg.by_category[cat] ?? 0) + amt;
      transactions.push({
        id: `demo-${txIdx++}`,
        booking_date: date,
        value_date: date,
        reference: '',
        text,
        currency: 'SEK',
        amount: amt,
        balance: bal[acct],
        approved,
        note: '',
        category: cat,
        account_id: acct,
      });
    };

    // 1. Salary on the 25th (or last weekday)
    const salaryDay = Math.min(25, lastDay);
    const salaryAmt = rng.int(2800000, 3500000);
    addAmt(
      'demo-checking',
      'Salary',
      salaryAmt,
      salaryDay,
      'Employer AB — Salary',
    );

    // 2. Fixed monthly bills (if month has progressed far enough)
    if (lastDay >= 5)
      addAmt(
        'demo-checking',
        'Leisure',
        -13900,
        Math.min(5, lastDay),
        'Netflix',
      );
    if (lastDay >= 8)
      addAmt(
        'demo-checking',
        'Leisure',
        -10900,
        Math.min(8, lastDay),
        'Spotify',
      );
    if (lastDay >= 3)
      addAmt(
        'demo-checking',
        'Household & Services',
        -rng.int(39900, 79900),
        Math.min(3, lastDay),
        'Telia',
      );
    if (lastDay >= 1)
      addAmt(
        'demo-checking',
        'Transport',
        -99500,
        Math.min(2, lastDay),
        'SL Månadskort',
      );

    // 3. Monthly transfer to savings
    if (lastDay >= 20) {
      const savingsAmt = rng.int(50000, 200000);
      addAmt(
        'demo-checking',
        'Savings & Investments',
        -savingsAmt,
        Math.min(20, lastDay),
        'Transfer to Savings',
      );
      addAmt(
        'demo-savings',
        'Savings & Investments',
        savingsAmt,
        Math.min(20, lastDay),
        'Transfer from Checking',
      );
    }

    // 4. Quarterly dividends (Jan, Apr, Jul, Oct)
    if ([1, 4, 7, 10].includes(curMonth) && lastDay >= 15) {
      const divAmt = rng.int(40000, 120000);
      addAmt(
        'demo-invest',
        'Other Income',
        divAmt,
        Math.min(15, lastDay),
        'Dividends — XACT',
      );
    }

    // 5. Electricity bill every other month (Jan, Mar, May, Jul, Sep, Nov)
    if (curMonth % 2 === 1 && lastDay >= 12) {
      addAmt(
        'demo-checking',
        'Household & Services',
        -rng.int(55000, 180000),
        Math.min(12, lastDay),
        'Vattenfall',
      );
    }

    // 6. 1-4 random expense transactions per day
    for (let day = 1; day <= lastDay; day++) {
      const nTx = rng.int(1, 4);
      for (let t = 0; t < nTx; t++) {
        const m = pickWeighted(rng);
        // Skip SL Månadskort — already handled
        if (m.name === 'SL Månadskort') continue;
        const amt = -rng.int(m.min, m.max);
        const approved = !isCurrentMonth || day < today.getDate() - 2;
        addAmt(m.acct, m.cat, amt, day, m.name, approved);
      }
    }

    // Snapshot balances at end of month
    agg.balance = { ...bal };
    aggMap[monthKey] = agg;

    // Advance month
    curMonth++;
    if (curMonth > 12) {
      curMonth = 1;
      curYear++;
    }
  }

  // Sort transactions newest-first (the generator adds them oldest-first)
  transactions.sort((a, b) => b.booking_date.localeCompare(a.booking_date));

  _cache = {
    transactions,
    aggregations: aggMap,
    models: [
      {
        version: 0,
        is_base: true,
        is_active: true,
        trained_at: 'shipped',
        samples_used: 0,
        epochs: 0,
        train_loss: null,
        train_accuracy: 0.81,
        val_loss: null,
        val_accuracy: 0.81,
        epoch_history: [],
      },
    ],
  };

  return _cache;
}
