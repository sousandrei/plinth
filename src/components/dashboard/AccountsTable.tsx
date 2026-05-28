import { Line, LineChart, ResponsiveContainer, Tooltip } from 'recharts';
import { Card, CardBody } from '@/components/ui/Card';
import type { AccountSeries } from '@/hooks/dashboard/types';

interface Props {
  accountSeries: AccountSeries[];
  currency?: string;
}

const TYPE_LABELS: Record<string, string> = {
  checking: 'Checking',
  savings: 'Savings',
  investment: 'Investment',
  credit: 'Credit',
  loan: 'Loan',
  other: 'Other',
};

const fmt = (value: number, currency: string): string =>
  new Intl.NumberFormat('sv-SE', {
    style: 'currency',
    currency,
    maximumFractionDigits: 0,
  }).format(value);

const fmtMonth = (ym: string): string => {
  const parts = ym.split('-');
  if (parts.length < 2) return ym;
  const year = Number(parts[0]);
  const month = Number(parts[1]);
  if (Number.isNaN(year) || Number.isNaN(month)) return ym;
  return new Date(year, month - 1).toLocaleString(undefined, {
    month: 'short',
    year: '2-digit',
  });
};

interface SparklineProps {
  history: { month: string; value: number }[];
  currency: string;
}

const Sparkline = ({
  history,
  currency,
}: SparklineProps): React.JSX.Element => (
  <ResponsiveContainer width="100%" height="100%">
    <LineChart data={history} margin={{ top: 2, right: 2, bottom: 2, left: 2 }}>
      <Tooltip
        formatter={(v) => [fmt(Number(v), currency), 'Balance']}
        labelFormatter={(_, payload) => {
          const month = payload?.[0]?.payload?.month;
          return month ? fmtMonth(String(month)) : '';
        }}
        contentStyle={{
          background: 'oklch(99.5% 0.002 80)',
          border: '1px solid oklch(84% 0.005 240)',
          borderRadius: 0,
          fontSize: 11,
          fontFamily: 'var(--font-mono)',
        }}
        itemStyle={{ color: 'oklch(8% 0.005 264)' }}
        labelStyle={{ color: 'oklch(44% 0.006 264)', marginBottom: 4 }}
      />
      <Line
        type="monotone"
        dataKey="value"
        stroke="oklch(50% 0.25 264)"
        strokeWidth={1.5}
        dot={false}
        activeDot={{ r: 2, strokeWidth: 0, fill: 'oklch(50% 0.25 264)' }}
      />
    </LineChart>
  </ResponsiveContainer>
);

export const AccountsTable = ({
  accountSeries,
  currency = 'SEK',
}: Props): React.JSX.Element => (
  <Card className="flex flex-col h-full overflow-hidden">
    <CardBody className="flex flex-col gap-0 flex-1 p-0">
      {/* Header */}
      <div className="px-5 pt-5 pb-3 border-b border-border-subtle">
        <span className="text-[10px] uppercase tracking-[0.18em] font-mono text-muted-foreground">
          Accounts
        </span>
      </div>

      {/* Rows */}
      <div className="flex-1 overflow-auto">
        {accountSeries.map(({ account, latestBalance, history }, i) => (
          <div
            key={account.id}
            className={`flex items-center px-5 py-3 ${
              i < accountSeries.length - 1
                ? 'border-b border-border-subtle'
                : ''
            }`}
          >
            {/* Name + type — fixed width */}
            <div className="w-40 shrink-0 min-w-0">
              <p className="text-sm font-medium text-foreground truncate">
                {account.name}
              </p>
              <p className="text-[10px] font-mono uppercase tracking-wider text-muted-foreground mt-0.5">
                {TYPE_LABELS[account.account_type] ?? account.account_type}
              </p>
            </div>

            {/* Sparkline — grows to fill */}
            <div className="flex-1 h-10 min-w-0">
              <Sparkline history={history} currency={currency} />
            </div>

            {/* Balance — fixed width, right-aligned */}
            <div className="w-32 shrink-0 text-right">
              <p className="text-sm font-mono font-semibold tabular-nums text-foreground">
                {fmt(latestBalance, currency)}
              </p>
            </div>
          </div>
        ))}

        {accountSeries.length === 0 && (
          <div className="px-5 py-8 text-center text-xs font-mono text-muted-foreground">
            No accounts
          </div>
        )}
      </div>
    </CardBody>
  </Card>
);
