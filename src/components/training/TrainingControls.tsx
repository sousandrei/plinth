import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Toggle } from '@/components/ui/Toggle';
import type { FinetuneConfig, FinetuneResult } from '@/types';

interface TrainingControlsProps {
  approvedCount: number;
  isTraining: boolean;
  result: FinetuneResult | null;
  error: string | null;
  onStart: (config: Partial<FinetuneConfig>) => void;
  onStop: () => void;
}

export function TrainingControls({
  approvedCount,
  isTraining,
  result,
  error,
  onStart,
  onStop,
}: TrainingControlsProps): React.JSX.Element {
  const [epochs, setEpochs] = useState(10);
  const [mode, setMode] = useState<'finetune' | 'scratch'>('finetune');
  const [stopping, setStopping] = useState(false);

  // Clear the stopping flag as soon as training finishes.
  useEffect(() => {
    if (!isTraining) setStopping(false);
  }, [isTraining]);

  const handleStop = () => {
    setStopping(true);
    onStop();
  };

  return (
    <Card>
      <CardHeader label="Fine-tune Model" meta={`${approvedCount} approved`} />
      <CardBody className="space-y-6">
        <div className="space-y-1.5 text-left">
          <label
            htmlFor="training-epochs"
            className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
          >
            Epochs
          </label>
          <input
            id="training-epochs"
            type="number"
            min={1}
            max={50}
            value={epochs}
            onChange={(e) => setEpochs(Math.max(1, Number(e.target.value)))}
            disabled={isTraining}
            className="w-24 bg-canvas border border-border-subtle px-3 py-2 text-sm font-mono text-foreground focus:outline-none focus:border-accent disabled:opacity-50"
          />
        </div>

        <Toggle
          options={[
            { value: 'finetune', label: 'Fine-tune' },
            { value: 'scratch', label: 'From scratch' },
          ]}
          value={mode}
          onValueChange={setMode}
          disabled={isTraining}
        />

        {error && <p className="text-xs font-mono text-expense">{error}</p>}

        {result && !isTraining && (
          <div className="text-xs font-mono text-muted-foreground space-y-0.5">
            <p>
              Saved as{' '}
              <span className="text-foreground">v{result.version}</span> —{' '}
              {result.samples_used} samples, {result.epochs_completed} epochs
            </p>
            <p>
              val accuracy{' '}
              <span className="text-growth">
                {(result.final_val_accuracy * 100).toFixed(1)}%
              </span>
            </p>
          </div>
        )}

        <div className="flex justify-end gap-2 pt-2 border-t border-border-subtle">
          {isTraining && (
            <Button
              variant="secondary"
              onClick={handleStop}
              disabled={stopping}
              className="px-6 rounded-none h-10 gap-2"
            >
              {stopping ? (
                <>
                  <Spinner size="sm" />
                  Stopping…
                </>
              ) : (
                'Stop'
              )}
            </Button>
          )}
          <Button
            variant="primary"
            onClick={() =>
              onStart({ epochs, from_scratch: mode === 'scratch' })
            }
            disabled={isTraining || approvedCount < 20}
            className="px-6 rounded-none h-10 gap-2"
          >
            {isTraining ? (
              <>
                <Spinner size="sm" />
                Training…
              </>
            ) : (
              'Start Training'
            )}
          </Button>
        </div>
      </CardBody>
    </Card>
  );
}
