import { Area, AreaChart, ResponsiveContainer, Tooltip } from 'recharts';
import type {
  NameType,
  ValueType,
} from 'recharts/types/component/DefaultTooltipContent';
import type { TooltipContentProps } from 'recharts/types/component/Tooltip';
import { Card, CardBody } from '@/components/ui/Card';
import type { NetWorthPoint } from '@/hooks/dashboard/types';
import { fmtMajor, fmtMonth } from '@/lib/format';

interface Props {
  series: NetWorthPoint[];
  latestNetWorth: number;
  currency?: string;
}

const NetWorthTooltip = ({
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
        <span className="inline-block w-1.5 h-1.5 rounded-full shrink-0 bg-[oklch(48%_0.19_145)]" />
        <span className="text-muted-foreground flex-1">Net worth</span>
        <span className="text-foreground ml-3">
          {fmtMajor(value, currency)}
        </span>
      </div>
    </div>
  );
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
              {fmtMajor(latestNetWorth, currency)}
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
                {fmtMajor(delta, currency)} last month
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
                content={(props) => (
                  <NetWorthTooltip {...props} currency={currency} />
                )}
                cursor={{
                  stroke: 'oklch(84% 0.005 240)',
                  strokeWidth: 1,
                  strokeDasharray: '3 3',
                }}
                wrapperStyle={{ zIndex: 10 }}
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
