import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import type { EmbedProgress, FinetuneProgress } from '@/types';

interface ProgressChartProps {
  history: FinetuneProgress[];
  totalEpochs: number;
  isTraining: boolean;
  embedProgress: EmbedProgress | null;
}

export function ProgressChart({
  history,
  totalEpochs,
  isTraining,
  embedProgress,
}: ProgressChartProps): React.JSX.Element {
  const isEmbedding = embedProgress !== null && embedProgress.total > 0;
  const meta = isEmbedding
    ? `Embedding ${embedProgress.current} / ${embedProgress.total}`
    : isTraining
      ? `Epoch ${history.length} / ${totalEpochs}`
      : history.length > 0
        ? `${history.length} epochs`
        : '';

  return (
    <Card>
      <CardHeader label="Training Progress" meta={meta} />
      <CardBody>
        {isEmbedding ? (
          <p className="text-xs font-mono text-muted-foreground py-8 text-center">
            Pre-computing MiniLM embeddings before the epoch loop…
          </p>
        ) : history.length === 0 ? (
          <p className="text-xs font-mono text-muted-foreground py-8 text-center">
            {isTraining ? 'Starting…' : 'No training run yet'}
          </p>
        ) : (
          <ResponsiveContainer width="100%" height={220}>
            <LineChart
              data={history}
              margin={{ top: 4, right: 4, bottom: 0, left: -24 }}
            >
              <CartesianGrid
                strokeDasharray="3 3"
                stroke="oklch(84% 0.005 240)"
              />
              <XAxis
                dataKey="epoch"
                tick={{ fontSize: 10, fontFamily: 'var(--font-mono)' }}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                domain={[0, 1]}
                tickFormatter={(v) => `${(v * 100).toFixed(0)}%`}
                tick={{ fontSize: 10, fontFamily: 'var(--font-mono)' }}
                tickLine={false}
                axisLine={false}
              />
              <Tooltip
                formatter={(v: unknown, name: unknown) => [
                  String(name).includes('loss')
                    ? Number(v).toFixed(4)
                    : `${(Number(v) * 100).toFixed(1)}%`,
                  String(name),
                ]}
                contentStyle={{
                  background: 'oklch(99.5% 0.002 80)',
                  border: '1px solid oklch(84% 0.005 240)',
                  borderRadius: 0,
                  fontSize: 11,
                  fontFamily: 'var(--font-mono)',
                }}
              />
              <Legend
                wrapperStyle={{ fontSize: 10, fontFamily: 'var(--font-mono)' }}
              />
              <Line
                type="monotone"
                dataKey="train_accuracy"
                name="train acc"
                stroke="oklch(48% 0.19 145)"
                strokeWidth={1.5}
                dot={false}
              />
              <Line
                type="monotone"
                dataKey="val_accuracy"
                name="val acc"
                stroke="oklch(52% 0.23 22)"
                strokeWidth={1.5}
                dot={false}
                strokeDasharray="4 2"
              />
            </LineChart>
          </ResponsiveContainer>
        )}
      </CardBody>
    </Card>
  );
}
