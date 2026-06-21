import { Bar, BarChart, Cell, ResponsiveContainer, Tooltip } from 'recharts';
import type {
  NameType,
  ValueType,
} from 'recharts/types/component/DefaultTooltipContent';
import type { TooltipContentProps } from 'recharts/types/component/Tooltip';
import { Card, CardBody } from '@/components/ui/Card';
import type { CashFlowPoint } from '@/hooks/dashboard/types';

interface Props {
  series: CashFlowPoint[];
  currency?: string;
}

const COLOR_POSITIVE = 'oklch(48% 0.19 145)';
const COLOR_NEGATIVE = 'oklch(52% 0.23 22)';

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

const CashFlowTooltip = ({
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
  const color = value >= 0 ? COLOR_POSITIVE : COLOR_NEGATIVE;

  return (
    <div className="bg-canvas-raised border border-border-muted text-[11px] font-mono px-2.5 py-2">
      {month && (
        <p className="text-muted-foreground mb-1.5 font-medium">
          {fmtMonth(String(month))}
        </p>
      )}
      <div className="flex items-center gap-1.5">
        <span
          className="inline-block w-1.5 h-1.5 rounded-full shrink-0"
          style={{ background: color }}
        />
        <span className="text-muted-foreground flex-1">Net</span>
        <span className="text-foreground ml-3">{fmt(value, currency)}</span>
      </div>
    </div>
  );
};

export const CashFlowChart = ({
  series,
  currency = 'SEK',
}: Props): React.JSX.Element => {
  const latest = series[series.length - 1];
  const positive = (latest?.net ?? 0) >= 0;

  return (
    <Card className="flex flex-col h-full overflow-hidden">
      <CardBody className="flex flex-col gap-0 flex-1 p-0">
        {/* Header */}
        <div className="px-5 pt-5 pb-4">
          <span className="text-[10px] uppercase tracking-[0.18em] font-mono text-muted-foreground">
            Monthly Cash Flow
          </span>

          <div className="mt-2">
            <span className="text-[2.25rem] font-semibold tracking-tight text-foreground leading-none tabular-nums">
              {latest ? fmt(latest.net, currency) : '—'}
            </span>
          </div>

          {latest && (
            <div className="mt-1.5 flex items-center gap-1.5">
              <span
                className={`inline-block w-1.5 h-1.5 rounded-full ${positive ? 'bg-growth' : 'bg-expense'}`}
              />
              <span
                className={`text-xs font-mono ${positive ? 'text-growth' : 'text-expense'}`}
              >
                {fmtMonth(latest.month)}
              </span>
            </div>
          )}
        </div>

        {/* Chart */}
        <div className="flex-1 min-h-[100px]">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart
              data={series}
              margin={{ top: 8, right: 0, bottom: 8, left: 0 }}
              barCategoryGap="30%"
            >
              <Tooltip
                content={(props) => (
                  <CashFlowTooltip {...props} currency={currency} />
                )}
                cursor={false}
                wrapperStyle={{ zIndex: 10 }}
              />
              <Bar
                dataKey="net"
                radius={[2, 2, 0, 0]}
                fillOpacity={0.85}
                activeBar={{ fillOpacity: 1, strokeWidth: 1 }}
              >
                {series.map((point) => (
                  <Cell
                    key={point.month}
                    fill={point.net >= 0 ? COLOR_POSITIVE : COLOR_NEGATIVE}
                    stroke={point.net >= 0 ? COLOR_POSITIVE : COLOR_NEGATIVE}
                  />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      </CardBody>
    </Card>
  );
};
