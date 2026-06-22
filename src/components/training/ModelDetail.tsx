import { ProgressChart } from '@/components/training/ProgressChart';
import type { ModelCard } from '@/types';

interface ModelDetailProps {
  model: ModelCard | null;
}

export function ModelDetail({
  model,
}: ModelDetailProps): React.JSX.Element | null {
  if (!model || model.epoch_history.length === 0) return null;

  return (
    <ProgressChart
      history={model.epoch_history}
      totalEpochs={model.epochs}
      isTraining={false}
      embedProgress={null}
    />
  );
}
