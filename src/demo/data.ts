/**
 * Deterministic demo data generator.
 * Uses a seeded LCG so the output is identical on every call / hot-reload.
 *
 * Design goals:
 *  - Net worth visibly grows over 5 years (salary growth + savings + ISK returns)
 *  - Realistic Swedish household: income > expenses every month
 *  - Sporadic income windfalls (freelance, tax refund, sold items)
 *  - Inflation gently raises expenses over time
 *  - 2–5 expense transactions per day, drawn from a weighted merchant pool
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
    space_id: 'demo',
  },
  {
    id: 'demo-savings',
    name: 'Demo Savings',
    currency: 'SEK',
    account_type: 'savings',
    account_source: 'import',
    color: '#10b981',
    space_id: 'demo',
  },
  {
    id: 'demo-invest',
    name: 'Demo ISK',
    currency: 'SEK',
    account_type: 'investment',
    account_source: 'import',
    color: '#8b5cf6',
    space_id: 'demo',
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
// Merchant pool
//
// Each entry defines one payee. The `weight` controls how often it appears
// in the daily draw. The `freq` field caps how many times per month it can
// fire naturally (0 = unlimited, handled as a fixed bill separately).
//
// Amounts are in öre (1 SEK = 100 öre).
// ---------------------------------------------------------------------------

type Merchant = {
  name: string;
  cat: string;
  min: number;
  max: number;
  weight: number;
};

const MERCHANTS: Merchant[] = [
  // Food & Drinks — frequent, low-to-mid amounts
  {
    name: 'ICA Maxi',
    cat: 'Food & Drinks',
    min: 20000,
    max: 65000,
    weight: 12,
  },
  { name: 'Willys', cat: 'Food & Drinks', min: 15000, max: 45000, weight: 10 },
  { name: 'Coop', cat: 'Food & Drinks', min: 15000, max: 48000, weight: 8 },
  { name: 'Lidl', cat: 'Food & Drinks', min: 10000, max: 30000, weight: 7 },
  {
    name: 'Espresso House',
    cat: 'Food & Drinks',
    min: 3500,
    max: 9000,
    weight: 9,
  },
  {
    name: "Wayne's Coffee",
    cat: 'Food & Drinks',
    min: 3000,
    max: 8000,
    weight: 7,
  },
  {
    name: 'Restaurang Pelikan',
    cat: 'Food & Drinks',
    min: 28000,
    max: 95000,
    weight: 3,
  },
  {
    name: 'Restaurang AG',
    cat: 'Food & Drinks',
    min: 40000,
    max: 130000,
    weight: 2,
  },
  {
    name: 'Systembolaget',
    cat: 'Food & Drinks',
    min: 12000,
    max: 35000,
    weight: 4,
  },

  // Transport — medium frequency
  { name: 'Bolt', cat: 'Transport', min: 6000, max: 20000, weight: 5 },
  {
    name: 'Taxi Stockholm',
    cat: 'Transport',
    min: 15000,
    max: 45000,
    weight: 2,
  },
  { name: 'Biltvätt', cat: 'Transport', min: 9900, max: 20000, weight: 2 },

  // Shopping — lower frequency, capped to avoid blowing the budget
  { name: 'H&M', cat: 'Shopping', min: 20000, max: 80000, weight: 3 },
  { name: 'Zalando', cat: 'Shopping', min: 25000, max: 100000, weight: 2 },
  { name: 'Amazon', cat: 'Shopping', min: 15000, max: 70000, weight: 3 },
  { name: 'Stadium', cat: 'Shopping', min: 35000, max: 130000, weight: 1 },
  { name: 'Åhlens', cat: 'Shopping', min: 20000, max: 100000, weight: 2 },
  { name: 'Elgiganten', cat: 'Shopping', min: 49900, max: 200000, weight: 1 },
  {
    name: 'IKEA',
    cat: 'Household & Services',
    min: 35000,
    max: 180000,
    weight: 1,
  },

  // Leisure — low frequency
  { name: 'SF Bio', cat: 'Leisure', min: 15000, max: 32000, weight: 2 },
  {
    name: 'Göteborgsoperan',
    cat: 'Leisure',
    min: 20000,
    max: 75000,
    weight: 1,
  },

  // Health & Beauty
  {
    name: 'Apoteket',
    cat: 'Health & Beauty',
    min: 7000,
    max: 35000,
    weight: 4,
  },
  {
    name: 'Gymkort',
    cat: 'Health & Beauty',
    min: 39900,
    max: 59900,
    weight: 2,
  },
  {
    name: 'Tandläkaren',
    cat: 'Health & Beauty',
    min: 45000,
    max: 180000,
    weight: 1,
  },
  {
    name: 'Vårdcentral',
    cat: 'Health & Beauty',
    min: 20000,
    max: 20000,
    weight: 1,
  },
];

const MERCHANT_WEIGHT_SUM = MERCHANTS.reduce((s, m) => s + m.weight, 0);

function pickMerchant(rng: Rng): Merchant {
  let r = rng.next() * MERCHANT_WEIGHT_SUM;
  for (const m of MERCHANTS) {
    r -= m.weight;
    if (r <= 0) return m;
  }
  return MERCHANTS[MERCHANTS.length - 1];
}

// ---------------------------------------------------------------------------
// Sporadic income events — fire probabilistically each day
// ---------------------------------------------------------------------------

type IncomeEvent = {
  text: string;
  cat: string;
  min: number;
  max: number;
  annualFreq: number; // expected occurrences per year
};

const INCOME_EVENTS: IncomeEvent[] = [
  {
    text: 'Freelance uppdrag',
    cat: 'Other Income',
    min: 500000,
    max: 1500000,
    annualFreq: 4,
  },
  {
    text: 'Blocket — Sålt möbler',
    cat: 'Other Income',
    min: 50000,
    max: 300000,
    annualFreq: 3,
  },
  {
    text: 'Swish från vän',
    cat: 'Other Income',
    min: 15000,
    max: 70000,
    annualFreq: 6,
  },
  {
    text: 'Återbetalning — Försäkring',
    cat: 'Other Income',
    min: 80000,
    max: 220000,
    annualFreq: 1,
  },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function pad(n: number, len = 2): string {
  return String(n).padStart(len, '0');
}

function dateStr(y: number, m: number, d: number): string {
  return `${y}-${pad(m)}-${pad(d)}`;
}

// Linear interpolate between `from` and `to` as `t` moves from 0 → `total`
function lerp(from: number, to: number, t: number, total: number): number {
  return from + (to - from) * Math.min(t / total, 1);
}

// ---------------------------------------------------------------------------
// Types
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

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

let _cache: GeneratedData | null = null;

export function getGeneratedData(): GeneratedData {
  if (_cache) return _cache;

  const rng = new Rng(42);
  const transactions: Transaction[] = [];

  // Start with a decent buffer so the account doesn't immediately go negative
  const bal: Record<string, number> = {
    'demo-checking': 8000000, // 80 000 SEK — a few months of salary in the account
    'demo-savings': 200000, // 2 000 SEK
    'demo-invest': 500000, // 5 000 SEK
  };

  const aggMap: Record<string, AggMonth> = {};

  const today = new Date();
  const start = new Date(today);
  start.setFullYear(today.getFullYear() - 5);

  const totalMonths = 5 * 12;
  let monthIndex = 0;
  let txIdx = 0;

  let curYear = start.getFullYear();
  let curMonth = start.getMonth() + 1;

  while (
    curYear < today.getFullYear() ||
    (curYear === today.getFullYear() && curMonth <= today.getMonth() + 1)
  ) {
    const monthKey = `${curYear}-${pad(curMonth)}`;
    const daysInMonth = new Date(curYear, curMonth, 0).getDate();
    const isCurrentMonth =
      curYear === today.getFullYear() && curMonth === today.getMonth() + 1;
    const lastDay = isCurrentMonth ? today.getDate() : daysInMonth;

    // Progress [0..1] through the 5-year window
    const progress = monthIndex / totalMonths;

    // Gentle inflation on variable expenses (~1.5 %/yr compounded)
    const inflFactor = 1.015 ** (monthIndex / 12);

    // Seasonal spending multiplier — Christmas (+35%), summer holiday (+20%), Jan austerity (-15%)
    const seasonMult =
      curMonth === 12
        ? 1.35
        : curMonth === 11
          ? 1.15
          : curMonth === 7
            ? 1.2
            : curMonth === 6
              ? 1.1
              : curMonth === 1
                ? 0.85
                : curMonth === 2
                  ? 0.9
                  : 1.0;

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

    // -- Salary (25th) --
    // Grows from ~38 000 → ~50 000 SEK/month over 5 years
    const salaryDay = Math.min(25, lastDay);
    const salaryMin = Math.round(lerp(3800000, 4800000, progress, 1));
    const salaryMax = Math.round(lerp(4200000, 5500000, progress, 1));
    const salaryAmt = rng.int(salaryMin, salaryMax);
    addAmt(
      'demo-checking',
      'Salary',
      salaryAmt,
      salaryDay,
      'Employer AB — Salary',
    );

    // -- Fixed monthly bills --
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
        -Math.round(rng.int(39900, 69900) * inflFactor),
        Math.min(3, lastDay),
        'Telia',
      );
    if (lastDay >= 2)
      addAmt(
        'demo-checking',
        'Transport',
        -99500,
        Math.min(2, lastDay),
        'SL Månadskort',
      );

    // -- Electricity (odd months) --
    if (curMonth % 2 === 1 && lastDay >= 12)
      addAmt(
        'demo-checking',
        'Household & Services',
        -Math.round(rng.int(45000, 150000) * inflFactor),
        Math.min(12, lastDay),
        'Vattenfall',
      );

    // -- Rent / mortgage (1st of month) --
    // Fixed large outflow — the biggest household expense
    if (lastDay >= 1) {
      const rent = Math.round(lerp(1100000, 1300000, progress, 1)); // 11 000–13 000 SEK
      addAmt(
        'demo-checking',
        'Household & Services',
        -rent,
        1,
        'Hyra — Lägenheten',
      );
    }

    // -- Savings transfer (20th) — only when checking has a healthy buffer --
    // ~6–10 % of salary, growing slightly over time
    if (lastDay >= 20 && bal['demo-checking'] > 2000000) {
      const rateMin = lerp(0.06, 0.08, progress, 1);
      const rateMax = lerp(0.08, 0.1, progress, 1);
      const rate = rateMin + rng.next() * (rateMax - rateMin);
      const savingsAmt = Math.round(salaryAmt * rate);
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

      // Monthly interest ~2.5 % annualised
      const interest = Math.round(bal['demo-savings'] * 0.00208);
      if (interest > 0)
        addAmt(
          'demo-savings',
          'Other Income',
          interest,
          lastDay,
          'Ränta — Sparkonto',
        );
    }

    // -- ISK monthly contribution (18th) — only when checking has buffer --
    // Grows from 1 500 → 4 000 SEK/month over 5 years
    if (lastDay >= 18 && bal['demo-checking'] > 3000000) {
      const iskMin = Math.round(lerp(150000, 350000, progress, 1));
      const iskMax = Math.round(lerp(250000, 500000, progress, 1));
      const iskAmt = rng.int(iskMin, iskMax);
      addAmt(
        'demo-checking',
        'Savings & Investments',
        -iskAmt,
        Math.min(18, lastDay),
        'ISK — Månadsspar',
      );
      addAmt(
        'demo-invest',
        'Savings & Investments',
        iskAmt,
        Math.min(18, lastDay),
        'ISK — Insättning',
      );

      // Market return ~6 % annualised with noise
      const marketRate = 0.00487 + (rng.next() - 0.5) * 0.003;
      const returnAmt = Math.round(bal['demo-invest'] * marketRate);
      if (returnAmt !== 0)
        addAmt(
          'demo-invest',
          'Other Income',
          returnAmt,
          lastDay,
          'Avkastning — ISK',
        );
    }

    // -- Quarterly dividends (Jan, Apr, Jul, Oct) --
    if ([1, 4, 7, 10].includes(curMonth) && lastDay >= 15) {
      const divAmt = rng.int(30000, 100000);
      addAmt(
        'demo-invest',
        'Other Income',
        divAmt,
        Math.min(15, lastDay),
        'Dividends — XACT',
      );
    }

    // -- Annual tax refund (April) --
    if (curMonth === 4 && lastDay >= 20)
      addAmt(
        'demo-checking',
        'Other Income',
        rng.int(250000, 800000),
        Math.min(20, lastDay),
        'Skatteverket — Skatteåterbäring',
      );

    // -- Daily variable expenses --
    // 0–5 transactions per day — wide variance so some days are quiet, some heavy.
    // Amount scaled by both inflation and the seasonal multiplier.
    for (let day = 1; day <= lastDay; day++) {
      const nTx = rng.int(0, 5);
      for (let t = 0; t < nTx; t++) {
        const m = pickMerchant(rng);
        const amt = -Math.round(
          rng.int(m.min, m.max) * inflFactor * seasonMult,
        );
        const approved = !isCurrentMonth || day < today.getDate() - 2;
        addAmt('demo-checking', m.cat, amt, day, m.name, approved);
      }

      // Sporadic income — each event fires with probability annualFreq / 365
      for (const ev of INCOME_EVENTS) {
        if (rng.next() < ev.annualFreq / 365) {
          const amt = rng.int(ev.min, ev.max);
          const approved = !isCurrentMonth || day < today.getDate() - 2;
          addAmt('demo-checking', ev.cat, amt, day, ev.text, approved);
        }
      }

      // Rare one-off spike expenses (~5 per year) — car repair, medical, holiday booking, etc.
      // These can single-handedly push a month into the red.
      if (rng.next() < 5 / 365) {
        const spikes = [
          {
            text: 'Bilverkstad — Service',
            cat: 'Transport',
            min: 400000,
            max: 1200000,
          },
          {
            text: 'Semester — Flights & Hotel',
            cat: 'Leisure',
            min: 600000,
            max: 2500000,
          },
          {
            text: 'Tandläkaren — Behandling',
            cat: 'Health & Beauty',
            min: 300000,
            max: 900000,
          },
          {
            text: 'IKEA — Möbler',
            cat: 'Household & Services',
            min: 500000,
            max: 1800000,
          },
          {
            text: 'Elgiganten — TV/Laptop',
            cat: 'Shopping',
            min: 600000,
            max: 1500000,
          },
          {
            text: 'Hemreparation',
            cat: 'Household & Services',
            min: 800000,
            max: 3000000,
          },
          {
            text: 'Present — Bröllop/Kalas',
            cat: 'Shopping',
            min: 200000,
            max: 600000,
          },
        ] as const;
        const spike = rng.pick(spikes);
        const amt = -rng.int(spike.min, spike.max);
        const approved = !isCurrentMonth || day < today.getDate() - 2;
        addAmt('demo-checking', spike.cat, amt, day, spike.text, approved);
      }
    }

    // Snapshot balances at end of month
    agg.balance = { ...bal };
    aggMap[monthKey] = agg;

    monthIndex++;
    curMonth++;
    if (curMonth > 12) {
      curMonth = 1;
      curYear++;
    }
  }

  // Sort transactions newest-first
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
