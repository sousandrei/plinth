import {
  Bar,
  BarChart,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from 'recharts';
import type {
  NameType,
  ValueType,
} from 'recharts/types/component/DefaultTooltipContent';
import type { TooltipContentProps } from 'recharts/types/component/Tooltip';
import { Card, CardBody } from '@/components/ui/Card';
import type { CategoryMeta, SpendingPoint } from '@/hooks/dashboard/types';
import { fmtMajor, fmtMonth } from '@/lib/format';

interface Props {
  series: SpendingPoint[];
  categories: CategoryMeta[];
  currency?: string;
}

const SpendingTooltip = ({
  active,
  payload,
  categories,
  currency,
}: TooltipContentProps<ValueType, NameType> & {
  categories: CategoryMeta[];
  currency: string;
}): React.JSX.Element | null => {
  if (!active || !payload?.length) return null;

  const month = payload[0]?.payload?.month;

  return (
    <div className="bg-canvas-raised border border-border-muted text-[11px] font-mono px-2.5 py-2">
      {month && (
        <p className="text-muted-foreground mb-1.5 font-medium">
          {fmtMonth(String(month))}
        </p>
      )}
      {[...payload].reverse().map((entry) => {
        const cat = categories.find((c) => c.name === entry.name);
        return (
          <div key={entry.name} className="flex items-center gap-1.5 mb-0.5">
            <span
              className="inline-block w-1.5 h-1.5 rounded-full shrink-0"
              style={{ background: cat?.color ?? 'oklch(60% 0.08 264)' }}
            />
            <span className="text-muted-foreground flex-1">{entry.name}</span>
            <span className="text-foreground ml-3">
              {fmtMajor(Number(entry.value), currency)}
            </span>
          </div>
        );
      })}
    </div>
  );
};

export const SpendingByCategory = ({
  series,
  categories,
  currency = 'SEK',
}: Props): React.JSX.Element => (
  <Card className="flex flex-col h-full overflow-hidden">
    <CardBody className="flex flex-col gap-0 flex-1 p-0">
      {/* Header */}
      <div className="px-5 pt-5 pb-4">
        <span className="text-[10px] uppercase tracking-[0.18em] font-mono text-muted-foreground">
          Spending by Category
        </span>
      </div>

      {/* Chart */}
      <div className="flex-1 min-h-[200px]">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={series}
            margin={{ top: 4, right: 24, bottom: 8, left: 24 }}
            barCategoryGap="25%"
          >
            <XAxis
              dataKey="month"
              tickFormatter={fmtMonth}
              tick={{
                fontSize: 10,
                fontFamily: 'var(--font-mono)',
                fill: 'oklch(44% 0.006 264)',
              }}
              axisLine={false}
              tickLine={false}
            />
            <Tooltip
              content={(props) => (
                <SpendingTooltip
                  {...props}
                  categories={categories}
                  currency={currency}
                />
              )}
              cursor={false}
              wrapperStyle={{ zIndex: 10 }}
            />
            <Legend
              iconType="circle"
              iconSize={6}
              wrapperStyle={{
                fontSize: 10,
                fontFamily: 'var(--font-mono)',
                paddingTop: 8,
                paddingBottom: 8,
              }}
            />
            {categories.map((cat) => (
              <Bar
                key={cat.name}
                dataKey={cat.name}
                stackId="spend"
                fill={cat.color}
                fillOpacity={0.85}
                activeBar={{
                  fillOpacity: 1,
                  stroke: cat.color,
                  strokeWidth: 1,
                }}
                radius={
                  cat.name === categories[categories.length - 1].name
                    ? [2, 2, 0, 0]
                    : [0, 0, 0, 0]
                }
              />
            ))}
          </BarChart>
        </ResponsiveContainer>
      </div>
    </CardBody>
  </Card>
);
