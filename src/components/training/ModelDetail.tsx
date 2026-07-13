import { ChartLineIcon } from '@phosphor-icons/react';

import { ProgressChart } from '@/components/training/ProgressChart';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import type { ModelCard } from '@/types';

interface ModelDetailProps {
  model: ModelCard | null;
}

export function ModelDetail({ model }: ModelDetailProps): React.JSX.Element {
  if (!model || model.epoch_history.length === 0) {
    return (
      <Card>
        <CardHeader label="Training Progress" />
        <CardBody>
          <div className="flex flex-col items-center gap-3 py-12 text-center">
            <ChartLineIcon
              size={32}
              weight="regular"
              className="text-muted-foreground"
            />
            <p className="text-xs font-mono text-muted-foreground">
              Select a model to view its training history
            </p>
          </div>
        </CardBody>
      </Card>
    );
  }

  return (
    <ProgressChart
      history={model.epoch_history}
      totalEpochs={model.epochs}
      isTraining={false}
      embedProgress={null}
    />
  );
}
