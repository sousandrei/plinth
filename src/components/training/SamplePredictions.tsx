import { ArrowClockwiseIcon } from '@phosphor-icons/react';

import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import type { TrainingSample } from '@/types';

interface SamplePredictionsProps {
  samples: TrainingSample[];
  isLoading: boolean;
  isClassifierReady: boolean;
  onRefresh: () => void;
}

export function SamplePredictions({
  samples,
  isLoading,
  isClassifierReady,
  onRefresh,
}: SamplePredictionsProps): React.JSX.Element {
  const correct = samples.filter(
    (s) => s.actual_category === s.predicted_category,
  );

  const renderBody = () => {
    if (!isClassifierReady) {
      return (
        <p className="text-xs font-mono text-muted-foreground px-6 py-8 text-center">
          Train a model to see predictions
        </p>
      );
    }

    if (isLoading) {
      return (
        <div className="flex flex-col items-center gap-2 py-10">
          <Spinner size="sm" />
          <p className="text-[10px] font-mono text-muted-foreground">
            Running predictions…
          </p>
        </div>
      );
    }

    if (samples.length === 0) {
      return (
        <p className="text-xs font-mono text-muted-foreground px-6 py-8 text-center">
          No approved transactions yet
        </p>
      );
    }

    return (
      <ul className="divide-y divide-border-subtle max-h-96 overflow-y-auto">
        {samples.map((s) => {
          const match = s.actual_category === s.predicted_category;
          return (
            <li key={s.id} className="px-6 py-3 flex flex-col gap-1">
              <div className="flex items-start justify-between gap-3">
                <span
                  className="text-xs text-foreground truncate"
                  title={s.text}
                >
                  {s.text}
                </span>
                <span className="text-[10px] font-mono text-muted-foreground shrink-0">
                  {s.booking_date}
                </span>
              </div>
              <div className="flex items-center gap-2 flex-wrap">
                <Badge variant="muted">{s.actual_category}</Badge>
                {!match && (
                  <>
                    <span className="text-[10px] font-mono text-muted-foreground">
                      →
                    </span>
                    <Badge variant="expense">{s.predicted_category}</Badge>
                  </>
                )}
                {match && <Badge variant="growth">✓</Badge>}
              </div>
            </li>
          );
        })}
      </ul>
    );
  };

  return (
    <Card>
      <CardHeader label="Sample Predictions" />
      <CardBody className="p-0">
        {isClassifierReady && (
          <div className="flex items-center justify-between px-6 py-4 border-b border-border-subtle">
            <span className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground">
              {isLoading
                ? 'Running…'
                : `${correct.length} / ${samples.length} correct`}
            </span>
            <Button
              variant="ghost"
              onClick={onRefresh}
              disabled={isLoading}
              className="h-auto py-1 px-3 text-[10px] font-mono gap-2"
            >
              <ArrowClockwiseIcon
                size={11}
                weight="bold"
                aria-hidden="true"
                className={isLoading ? 'animate-spin' : ''}
              />
              Refresh
            </Button>
          </div>
        )}
        {renderBody()}
      </CardBody>
    </Card>
  );
}
