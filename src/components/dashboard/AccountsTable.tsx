import { Line, LineChart, ResponsiveContainer, Tooltip } from 'recharts';
import type {
  NameType,
  ValueType,
} from 'recharts/types/component/DefaultTooltipContent';
import type { TooltipContentProps } from 'recharts/types/component/Tooltip';
import { Card, CardBody } from '@/components/ui/Card';
import type { AccountSeries } from '@/hooks/dashboard/types';
import { fmtMajor, fmtMajorOrDash, fmtMonth } from '@/lib/format';

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

interface SparklineProps {
  history: { month: string; value: number }[];
  currency: string;
}

const SparklineTooltip = ({
  active,
  payload,
  currency,
}: TooltipContentProps<ValueType, NameType> & {
  currency: string;
}): React.JSX.Element | null => {
  if (!active || !payload?.length) return null;

  const entry = payload[0];
  const month = entry?.payload?.month;
  const value = Number(entry?.value ?? 0);

  return (
    <div className="bg-canvas-raised border border-border-muted text-[11px] font-mono px-2.5 py-2">
      {month && (
        <p className="text-muted-foreground mb-1.5 font-medium">
          {fmtMonth(String(month))}
        </p>
      )}
      <div className="flex items-center gap-1.5">
        <span className="inline-block w-1.5 h-1.5 rounded-full shrink-0 bg-[oklch(50%_0.25_264)]" />
        <span className="text-muted-foreground flex-1">Balance</span>
        <span className="text-foreground ml-3">
          {fmtMajor(value, currency)}
        </span>
      </div>
    </div>
  );
};

const Sparkline = ({
  history,
  currency,
}: SparklineProps): React.JSX.Element => (
  <ResponsiveContainer width="100%" height="100%">
    <LineChart data={history} margin={{ top: 2, right: 2, bottom: 2, left: 2 }}>
      <Tooltip
        content={(props) => <SparklineTooltip {...props} currency={currency} />}
        cursor={false}
        wrapperStyle={{ zIndex: 10 }}
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
                {fmtMajorOrDash(latestBalance, currency)}
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
