import { Pie, PieChart, ResponsiveContainer, Tooltip } from 'recharts';
import { Card, CardBody } from '@/components/ui/Card';
import type { AllocationPoint } from '@/hooks/dashboard/types';
import type { Account } from '@/types';

interface Props {
  series: AllocationPoint[];
  accounts: Account[];
  currency?: string;
}

const fmt = (value: number, currency: string): string =>
  new Intl.NumberFormat('sv-SE', {
    style: 'currency',
    currency,
    maximumFractionDigits: 0,
  }).format(value);

const fmtPct = (value: number): string =>
  new Intl.NumberFormat('sv-SE', {
    style: 'percent',
    maximumFractionDigits: 1,
  }).format(value);

export const NetWorthAllocation = ({
  series,
  accounts,
  currency = 'SEK',
}: Props): React.JSX.Element => {
  const latest = series[series.length - 1];

  const slices = accounts
    .map((account) => ({
      account,
      value: Number(latest?.[account.id] ?? 0),
      color: account.color,
      fill: account.color,
      fillOpacity: 0.9,
    }))
    .filter((s) => s.value > 0);

  const total = slices.reduce((sum, s) => sum + s.value, 0);

  return (
    <Card className="flex flex-col h-full overflow-hidden">
      <CardBody className="flex flex-col gap-0 flex-1 p-0">
        {/* Header */}
        <div className="px-5 pt-5 pb-3">
          <span className="text-[10px] uppercase tracking-[0.18em] font-mono text-muted-foreground">
            Allocation
          </span>
        </div>

        {/* Pie + legend side by side */}
        <div className="flex flex-1 min-h-0 items-center gap-3 px-4 pb-4">
          {/* Pie */}
          <div className="shrink-0" style={{ width: 120, height: 120 }}>
            <ResponsiveContainer width="100%" height="100%">
              <PieChart>
                <Tooltip
                  formatter={(v, name) => {
                    const slice = slices.find((s) => s.account.id === name);
                    return [
                      fmt(Number(v), currency),
                      slice?.account.name ?? String(name),
                    ];
                  }}
                  contentStyle={{
                    background: 'oklch(99.5% 0.002 80)',
                    border: '1px solid oklch(84% 0.005 240)',
                    borderRadius: 0,
                    fontSize: 11,
                    fontFamily: 'var(--font-mono)',
                  }}
                  itemStyle={{ color: 'oklch(8% 0.005 264)' }}
                />
                <Pie
                  data={slices}
                  dataKey="value"
                  nameKey={(s) => (s as (typeof slices)[0]).account.id}
                  cx="50%"
                  cy="50%"
                  innerRadius={32}
                  outerRadius={54}
                  strokeWidth={2}
                  stroke="oklch(99.5% 0.002 80)"
                  paddingAngle={2}
                />
              </PieChart>
            </ResponsiveContainer>
          </div>

          {/* Legend */}
          <div className="flex flex-col gap-2 flex-1 min-w-0">
            {slices.map((slice) => (
              <div
                key={slice.account.id}
                className="flex items-center gap-1.5 min-w-0"
              >
                <span
                  className="inline-block w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: slice.color }}
                />
                <div className="flex-1 min-w-0">
                  <p className="text-[11px] font-medium text-foreground truncate leading-none">
                    {slice.account.name}
                  </p>
                  <p className="text-[10px] font-mono text-muted-foreground mt-0.5">
                    {fmt(slice.value, currency)}
                    <span className="ml-1 text-muted-foreground/60">
                      {fmtPct(slice.value / total)}
                    </span>
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </CardBody>
    </Card>
  );
};
