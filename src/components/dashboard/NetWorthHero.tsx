import { Area, AreaChart, ResponsiveContainer, Tooltip } from 'recharts';
import { Card, CardBody } from '@/components/ui/Card';
import type { NetWorthPoint } from '@/hooks/dashboard/types';

interface Props {
  series: NetWorthPoint[];
  latestNetWorth: number;
  currency?: string;
}

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

export const NetWorthHero = ({
  series,
  latestNetWorth,
  currency = 'SEK',
}: Props): React.JSX.Element => {
  const hasPair = series.length >= 2;
  const prev = hasPair ? series[series.length - 2].value : latestNetWorth;
  const delta = latestNetWorth - prev;
  const positive = delta >= 0;

  return (
    <Card className="flex flex-col h-full overflow-hidden">
      <CardBody className="flex flex-col gap-0 flex-1 p-0">
        {/* Header area */}
        <div className="px-5 pt-5 pb-4">
          <span className="text-[10px] uppercase tracking-[0.18em] font-mono text-muted-foreground">
            Net Worth
          </span>

          <div className="mt-2 flex items-baseline gap-3">
            <span className="text-[2.25rem] font-semibold tracking-tight text-foreground leading-none tabular-nums">
              {fmt(latestNetWorth, currency)}
            </span>
          </div>

          {series.length > 1 && (
            <div className="mt-1.5 flex items-center gap-1.5">
              <span
                className={`inline-block w-1.5 h-1.5 rounded-full ${positive ? 'bg-growth' : 'bg-expense'}`}
              />
              <span
                className={`text-xs font-mono ${positive ? 'text-growth' : 'text-expense'}`}
              >
                {positive ? '+' : ''}
                {fmt(delta, currency)} last month
              </span>
            </div>
          )}
        </div>

        {/* Chart — bleeds to card edges, bottom-flush */}
        <div className="flex-1 min-h-[100px]">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart
              data={series}
              margin={{ top: 8, right: 0, bottom: 0, left: 0 }}
            >
              <defs>
                <linearGradient id="nw-fill" x1="0" y1="0" x2="0" y2="1">
                  <stop
                    offset="0%"
                    stopColor="oklch(48% 0.19 145)"
                    stopOpacity={0.12}
                  />
                  <stop
                    offset="100%"
                    stopColor="oklch(48% 0.19 145)"
                    stopOpacity={0}
                  />
                </linearGradient>
              </defs>
              <Tooltip
                formatter={(v) => [fmt(Number(v), currency), 'Net worth']}
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
                labelStyle={{
                  color: 'oklch(44% 0.006 264)',
                  marginBottom: 4,
                }}
                cursor={{
                  stroke: 'oklch(84% 0.005 240)',
                  strokeWidth: 1,
                  strokeDasharray: '3 3',
                }}
              />
              <Area
                type="monotone"
                dataKey="value"
                stroke="oklch(48% 0.19 145)"
                strokeWidth={1.5}
                fill="url(#nw-fill)"
                dot={false}
                activeDot={{
                  r: 3,
                  fill: 'oklch(48% 0.19 145)',
                  strokeWidth: 0,
                }}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </CardBody>
    </Card>
  );
};
