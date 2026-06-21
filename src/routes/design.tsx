import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { AccountsTable } from '@/components/dashboard/AccountsTable';
import { CashFlowChart } from '@/components/dashboard/CashFlowChart';
import { NetWorthAllocation } from '@/components/dashboard/NetWorthAllocation';
import { NetWorthHero } from '@/components/dashboard/NetWorthHero';
import { SpendingByCategory } from '@/components/dashboard/SpendingByCategory';
import { Avatar } from '@/components/ui/Avatar';
import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { DatePicker } from '@/components/ui/DatePicker';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import { Pagination } from '@/components/ui/Pagination';
import { Select } from '@/components/ui/Select';
import { Separator } from '@/components/ui/Separator';
import { Spinner } from '@/components/ui/Spinner';
import { Switch } from '@/components/ui/Switch';
import {
  Table,
  TableBody,
  TableHead,
  TableRow,
  Td,
  TdMono,
  Th,
} from '@/components/ui/Table';
import { Tabs } from '@/components/ui/Tabs';
import { Toggle } from '@/components/ui/Toggle';
import { Tooltip } from '@/components/ui/Tooltip';
import { categoryChipStyle, updateCategoryColors } from '@/lib/category-color';

// Seed the color map so category chips render with their real colours on this page
updateCategoryColors([
  { name: 'Food & Drinks', color: '#f59e0b' },
  { name: 'Transport', color: '#3b82f6' },
  { name: 'Shopping', color: '#8b5cf6' },
  { name: 'Leisure', color: '#ec4899' },
  { name: 'Health & Beauty', color: '#10b981' },
  { name: 'Household & Services', color: '#64748b' },
  { name: 'Salary', color: '#22c55e' },
  { name: 'Other Income', color: '#a3e635' },
  { name: 'Savings & Investments', color: '#06b6d4' },
  { name: 'Other', color: '#6b7280' },
]);

export const Route = createFileRoute('/design')({
  component: TestPage,
});

const TRANSACTIONS = [
  {
    id: '001',
    date: '2026-05-01',
    text: 'Löneinsättning',
    category: 'Salary',
    amount: 45000,
    balance: 82340,
  },
  {
    id: '002',
    date: '2026-05-03',
    text: 'ICA Maxi Solna',
    category: 'Food & Drinks',
    amount: -1243,
    balance: 81097,
  },
  {
    id: '003',
    date: '2026-05-07',
    text: 'Spotify Premium',
    category: 'Leisure',
    amount: -129,
    balance: 80968,
  },
  {
    id: '004',
    date: '2026-05-12',
    text: 'SL Månadskort',
    category: 'Transport',
    amount: -990,
    balance: 79978,
  },
];

const CATEGORIES = [
  { value: 'food', label: 'Food & Drinks' },
  { value: 'transport', label: 'Transport' },
  { value: 'shopping', label: 'Shopping' },
  { value: 'leisure', label: 'Leisure' },
  { value: 'health', label: 'Health & Beauty' },
];

const fmt = (n: number): string =>
  (n / 100).toLocaleString('sv-SE', { minimumFractionDigits: 2 });

const NET_WORTH_SERIES = [
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

const CASH_FLOW_SERIES = [
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

const ACCOUNT_SERIES = [
  {
    account: {
      id: 'a1',
      name: 'SEB Lönekonto',
      currency: 'SEK',
      account_type: 'checking',
      account_source: 'import',
      color: '#3b82f6',
      space_id: 'demo',
    },
    latestBalance: 82340,
    history: makeHistory([
      61000, 65000, 70000, 68000, 72000, 75000, 78000, 76000, 80000, 82340,
    ]),
  },
  {
    account: {
      id: 'a2',
      name: 'SEB Sparkonto',
      currency: 'SEK',
      account_type: 'savings',
      account_source: 'import',
      color: '#10b981',
      space_id: 'demo',
    },
    latestBalance: 150000,
    history: makeHistory([
      120000, 122000, 125000, 127000, 130000, 133000, 138000, 141000, 146000,
      150000,
    ]),
  },
  {
    account: {
      id: 'a3',
      name: 'Avanza ISK',
      currency: 'SEK',
      account_type: 'investment',
      account_source: 'import',
      color: '#8b5cf6',
      space_id: 'demo',
    },
    latestBalance: 301660,
    history: makeHistory([
      239000, 251000, 256000, 250000, 261000, 270000, 276000, 270000, 285000,
      301660,
    ]),
  },
];

const ALLOC_ACCOUNTS = ACCOUNT_SERIES.map((s) => s.account);

const ALLOC_SERIES = MONTHS.map((month, i) => ({
  month,
  a1: ACCOUNT_SERIES[0].history[i].value,
  a2: ACCOUNT_SERIES[1].history[i].value,
  a3: ACCOUNT_SERIES[2].history[i].value,
}));

const SPENDING_CATEGORIES = [
  { name: 'Groceries', color: 'oklch(58% 0.19 145)' },
  { name: 'Transport', color: 'oklch(50% 0.25 264)' },
  { name: 'Dining', color: 'oklch(70% 0.19 75)' },
  { name: 'Subscriptions', color: 'oklch(52% 0.23 22)' },
  { name: 'Shopping', color: 'oklch(55% 0.18 310)' },
];

const SPENDING_SERIES = MONTHS.map((month) => ({
  month,
  Groceries: 3200 + Math.trunc(Math.random() * 800),
  Transport: 990,
  Dining: 1200 + Math.trunc(Math.random() * 600),
  Subscriptions: 400,
  Shopping: 800 + Math.trunc(Math.random() * 1200),
}));

function TestPage(): React.JSX.Element {
  const [inputVal, setInputVal] = useState('');
  const [selectVal, setSelectVal] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [activeTab, setActiveTab] = useState('overview');
  const [toggleVal, setToggleVal] = useState<'a' | 'b'>('a');
  const [switchVal, setSwitchVal] = useState(false);
  const [dateFrom, setDateFrom] = useState('');
  const [dateTo, setDateTo] = useState('');
  const [page, setPage] = useState(0);

  return (
    <div className="max-w-[1200px] mx-auto px-6 py-10 space-y-12">
      {/* ── Buttons ── */}
      <section>
        <SectionLabel label="Buttons" code="Button" />
        <div className="flex flex-wrap items-center gap-4 mt-4">
          <Button variant="primary">Primary Action</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="primary" disabled>
            Disabled
          </Button>
        </div>
      </section>
      <Separator />
      {/* ── Toggle ── */}
      <section>
        <SectionLabel label="Toggle" code="Toggle" />
        <div className="mt-4 flex flex-wrap items-center gap-6">
          <Toggle
            options={[
              { value: 'a', label: 'Option A' },
              { value: 'b', label: 'Option B' },
            ]}
            value={toggleVal}
            onValueChange={setToggleVal}
          />
          <Toggle
            options={[
              { value: 'a', label: 'Option A' },
              { value: 'b', label: 'Option B' },
            ]}
            value="a"
            onValueChange={() => {}}
            disabled
          />
        </div>
        <p className="mt-3 text-xs font-mono text-muted-foreground">
          Selected: {toggleVal}
        </p>
      </section>
      <Separator />
      {/* ── Switch ── */}
      <section>
        <SectionLabel label="Switch" code="Switch" />
        <div className="mt-4 flex flex-wrap items-center gap-6">
          <Switch checked={switchVal} onCheckedChange={setSwitchVal} />
          <Switch checked={true} onCheckedChange={() => {}} disabled />
          <Switch checked={false} onCheckedChange={() => {}} disabled />
        </div>
        <p className="mt-3 text-xs font-mono text-muted-foreground">
          Checked: {String(switchVal)}
        </p>
      </section>
      <Separator />
      {/* ── Badges ── */}
      <section>
        <SectionLabel label="Badges" code="Badge" />
        <div className="flex flex-wrap items-center gap-3 mt-4">
          <Badge variant="default">Default</Badge>
          <Badge variant="accent">Cobalt</Badge>
          <Badge variant="muted">Muted</Badge>
          <Badge variant="growth">+12.4%</Badge>
          <Badge variant="expense">−3.1%</Badge>
          <Badge variant="highlight">Premium</Badge>
        </div>
      </section>
      <Separator />
      {/* ── Input ── */}
      <section>
        <SectionLabel label="Input" code="Input" />
        <div className="mt-4 max-w-sm space-y-3">
          <Input
            placeholder="Search transactions…"
            value={inputVal}
            onChange={(e) => setInputVal(e.target.value)}
          />
          <Input placeholder="Disabled input" disabled />
        </div>
      </section>
      <Separator />
      {/* ── Select ── */}
      <section>
        <SectionLabel label="Select" code="Select" />
        <div className="mt-4 max-w-xs">
          <Select
            options={CATEGORIES}
            value={selectVal ?? undefined}
            onValueChange={setSelectVal}
            placeholder="Choose category…"
          />
          {selectVal && (
            <p className="mt-2 text-xs font-mono text-muted-foreground">
              Selected: {selectVal}
            </p>
          )}
        </div>
      </section>
      <Separator />
      {/* ── Tooltip ── */}
      <section>
        <SectionLabel label="Tooltip" code="Tooltip" />
        <div className="mt-4 flex gap-6 items-center">
          <Tooltip content="Intellectual Cobalt — primary accent">
            <Button variant="primary">Hover for tooltip</Button>
          </Tooltip>
          <Tooltip content="SEK 45,000.00 gross salary before tax deductions">
            <span className="font-mono text-sm underline decoration-dotted cursor-default">
              45 000,00
            </span>
          </Tooltip>
        </div>
      </section>
      <Separator />
      {/* ── Dialog ── */}
      <section>
        <SectionLabel label="Dialog" code="Dialog" />
        <div className="mt-4">
          <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
            <DialogTrigger
              render={<Button variant="secondary">Open Dialog</Button>}
            />
            <DialogContent
              title="Confirm Action"
              description="This action cannot be undone. All selected transactions will be marked as approved."
            >
              <div className="mt-6 flex gap-3 justify-end">
                <DialogClose
                  render={<Button variant="secondary">Cancel</Button>}
                />
                <DialogClose
                  render={<Button variant="primary">Confirm</Button>}
                />
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </section>
      <Separator />
      {/* ── Cards ── */}
      <section>
        <SectionLabel label="Cards" code="Card / CardHeader / CardBody" />
        <div className="mt-4 grid grid-cols-1 md:grid-cols-3 gap-6">
          <Card>
            <CardHeader label="Net Worth" meta="SEC-01" />
            <CardBody>
              <p className="text-3xl font-mono font-semibold tabular-nums">
                823 400,00
              </p>
              <p className="text-xs font-mono text-muted-foreground mt-1">
                SEK
              </p>
              <Badge variant="growth" className="mt-3">
                +4.2% this month
              </Badge>
            </CardBody>
          </Card>

          <Card>
            <CardHeader label="Cash Flow" meta="MAY 2026" />
            <CardBody>
              <p className="text-3xl font-mono font-semibold tabular-nums text-growth">
                +42 638,00
              </p>
              <p className="text-xs font-mono text-muted-foreground mt-1">
                SEK
              </p>
              <Badge variant="accent" className="mt-3">
                Approved
              </Badge>
            </CardBody>
          </Card>

          <Card>
            <CardHeader label="Expenses" meta="MAY 2026" />
            <CardBody>
              <p className="text-3xl font-mono font-semibold tabular-nums text-expense">
                −2 362,00
              </p>
              <p className="text-xs font-mono text-muted-foreground mt-1">
                SEK
              </p>
              <Badge variant="expense" className="mt-3">
                4 items
              </Badge>
            </CardBody>
          </Card>
        </div>
      </section>
      <Separator />
      {/* ── Spinner ── */}
      <section>
        <SectionLabel label="Spinner" code="Spinner" />
        <div className="mt-4 flex items-center gap-8">
          {(['sm', 'md', 'lg'] as const).map((size) => (
            <div key={size} className="flex flex-col items-center gap-2">
              <Spinner size={size} />
              <span className="text-xs font-mono text-muted-foreground">
                {size}
              </span>
            </div>
          ))}
        </div>
      </section>
      <Separator />
      {/* ── Category Colors ── */}
      <section>
        <SectionLabel label="Category Colors" code="categoryChipStyle(name)" />
        <div className="mt-4 flex flex-wrap gap-2">
          {[
            'Food & Drinks',
            'Transport',
            'Shopping',
            'Leisure',
            'Health & Beauty',
            'Household & Services',
            'Salary',
            'Other Income',
            'Savings & Investments',
            'Other',
          ].map((cat) => (
            <span
              key={cat}
              className="inline-flex items-center px-2 py-0.5 text-xs uppercase tracking-widest font-mono border"
              style={categoryChipStyle(cat)}
            >
              {cat}
            </span>
          ))}
        </div>
      </section>
      <Separator />
      {/* ── Table ── */}
      <section>
        <SectionLabel label="Table" code="Table / Th / Td / TdMono" />
        <div className="mt-4 border border-border-muted">
          <Table>
            <TableHead>
              <TableRow>
                <Th>Date</Th>
                <Th>Description</Th>
                <Th>Category</Th>
                <Th className="text-right">Amount (SEK)</Th>
                <Th className="text-right">Balance (SEK)</Th>
              </TableRow>
            </TableHead>
            <TableBody>
              {TRANSACTIONS.map((tx) => (
                <TableRow key={tx.id}>
                  <TdMono className="text-muted-foreground">{tx.date}</TdMono>
                  <Td>{tx.text}</Td>
                  <Td>
                    <span
                      className="inline-flex items-center px-2 py-0.5 text-xs uppercase tracking-widest font-mono border"
                      style={categoryChipStyle(tx.category)}
                    >
                      {tx.category}
                    </span>
                  </Td>
                  <TdMono
                    className={`text-right ${tx.amount < 0 ? 'text-expense' : 'text-growth'}`}
                  >
                    {tx.amount > 0 ? '+' : ''}
                    {fmt(tx.amount)}
                  </TdMono>
                  <TdMono className="text-right">{fmt(tx.balance)}</TdMono>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </section>
      <Separator />
      {/* ── Tabs ── */}
      <section>
        <SectionLabel
          label="Tabs"
          code="Tabs.Root / Tabs.List / Tabs.Tab / Tabs.Panel"
        />
        <div className="mt-4 border border-border-muted">
          <Tabs.Root value={activeTab} onValueChange={setActiveTab}>
            <Tabs.List>
              <Tabs.Tab value="overview">Overview</Tabs.Tab>
              <Tabs.Tab value="transactions">Transactions</Tabs.Tab>
              <Tabs.Tab value="settings">Settings</Tabs.Tab>
            </Tabs.List>
            <Tabs.Panel value="overview" className="p-6">
              <p className="text-sm text-muted-foreground">
                Overview panel — summary of account health, recent activity, and
                key metrics.
              </p>
            </Tabs.Panel>
            <Tabs.Panel value="transactions" className="p-6">
              <p className="text-sm text-muted-foreground">
                Transactions panel — full ledger with filters, search, and
                pagination.
              </p>
            </Tabs.Panel>
            <Tabs.Panel value="settings" className="p-6">
              <p className="text-sm text-muted-foreground">
                Settings panel — currency, locale, notification preferences.
              </p>
            </Tabs.Panel>
          </Tabs.Root>
        </div>
      </section>
      <Separator />
      {/* ── DatePicker ── */}
      <section>
        <SectionLabel label="DatePicker" code="DatePicker" />
        <div className="mt-4 flex items-center gap-3">
          <DatePicker
            value={dateFrom}
            onChange={setDateFrom}
            placeholder="From…"
          />
          <span className="text-xs font-mono text-muted-foreground">→</span>
          <DatePicker value={dateTo} onChange={setDateTo} placeholder="To…" />
        </div>
        {(dateFrom || dateTo) && (
          <p className="mt-3 text-xs font-mono text-muted-foreground">
            {dateFrom || '—'} → {dateTo || '—'}
          </p>
        )}
      </section>
      <Separator />
      {/* ── Separator itself ── */}
      <section>
        <SectionLabel label="Separator" code="Separator" />
        <div className="mt-4 space-y-4">
          <Separator orientation="horizontal" />
          <div className="flex items-center gap-4 h-8">
            <span className="text-xs font-mono text-muted-foreground">
              Left
            </span>
            <Separator orientation="vertical" />
            <span className="text-xs font-mono text-muted-foreground">
              Right
            </span>
          </div>
        </div>
      </section>
      <Separator />
      {/* ── Avatar ── */}
      <section>
        <SectionLabel label="Avatar" code="Avatar" />
        <div className="mt-4 flex flex-wrap items-center gap-4">
          <Avatar name="Anna Svensson" />
          <Avatar name="Bo Karlsson" />
          <Avatar name="X" />
          <Avatar name="Demo User" onClick={() => {}} />
        </div>
      </section>
      <Separator />
      {/* ── Pagination ── */}
      <section>
        <SectionLabel label="Pagination" code="Pagination" />
        <div className="mt-4 max-w-sm space-y-4">
          <Pagination page={page} pageCount={8} onPageChange={setPage} />
          <p className="text-xs font-mono text-muted-foreground">
            Current page: {page + 1}
          </p>
          <Pagination page={0} pageCount={1} onPageChange={() => {}} />
          <p className="text-xs font-mono text-muted-foreground">
            pageCount=1 → renders nothing
          </p>
        </div>
      </section>
      <Separator />
      {/* ── NetWorthHero ── */}{' '}
      <section>
        <SectionLabel label="NetWorthHero" code="dashboard/NetWorthHero" />
        <div className="mt-4 max-w-md h-56">
          <NetWorthHero
            series={NET_WORTH_SERIES}
            latestNetWorth={NET_WORTH_SERIES[NET_WORTH_SERIES.length - 1].value}
            currency="SEK"
          />
        </div>
      </section>
      <Separator />
      {/* ── CashFlowChart ── */}
      <section>
        <SectionLabel label="CashFlowChart" code="dashboard/CashFlowChart" />
        <div className="mt-4 max-w-md h-56">
          <CashFlowChart series={CASH_FLOW_SERIES} currency="SEK" />
        </div>
      </section>
      <Separator />
      {/* ── AccountsTable ── */}
      <section>
        <SectionLabel label="AccountsTable" code="dashboard/AccountsTable" />
        <div className="mt-4 max-w-lg">
          <AccountsTable accountSeries={ACCOUNT_SERIES} currency="SEK" />
        </div>
      </section>
      <Separator />
      {/* ── NetWorthAllocation ── */}
      <section>
        <SectionLabel
          label="NetWorthAllocation"
          code="dashboard/NetWorthAllocation"
        />
        <div className="mt-4 max-w-xs h-56">
          <NetWorthAllocation
            series={ALLOC_SERIES}
            accounts={ALLOC_ACCOUNTS}
            currency="SEK"
          />
        </div>
      </section>
      <Separator />
      {/* ── SpendingByCategory ── */}
      <section>
        <SectionLabel
          label="SpendingByCategory"
          code="dashboard/SpendingByCategory"
        />
        <div className="mt-4 h-72">
          <SpendingByCategory
            series={SPENDING_SERIES}
            categories={SPENDING_CATEGORIES}
            currency="SEK"
          />
        </div>
      </section>
    </div>
  );
}

interface SectionLabelProps {
  label: string;
  code: string;
}

const SectionLabel = ({
  label,
  code,
}: SectionLabelProps): React.JSX.Element => (
  <div className="flex items-baseline gap-3">
    <h2 className="text-xs uppercase tracking-widest font-mono font-semibold">
      {label}
    </h2>
    <span className="text-xs font-mono text-muted-foreground">{code}</span>
  </div>
);
