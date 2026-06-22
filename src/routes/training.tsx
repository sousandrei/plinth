import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { getTrainingDevice } from '@/api/training';
import { MiniLmSetupBanner } from '@/components/training/MiniLmSetupBanner';
import { ModelCardList } from '@/components/training/ModelCardList';
import { ModelDetail } from '@/components/training/ModelDetail';
import { ProgressChart } from '@/components/training/ProgressChart';
import { SamplePredictions } from '@/components/training/SamplePredictions';
import { TrainingControls } from '@/components/training/TrainingControls';
import { useTraining } from '@/hooks/useTraining';
import type { ModelCard } from '@/types';

export const Route = createFileRoute('/training')({
  component: TrainingPage,
});

function TrainingPage(): React.JSX.Element {
  const [selectedVersion, setSelectedVersion] = useState<number | null>(null);
  const training = useTraining(selectedVersion);

  // Fetch active compile-time training hardware
  const { data: device = 'CPU' } = useQuery({
    queryKey: ['trainingDevice'],
    queryFn: getTrainingDevice,
  });

  const totalEpochs =
    training.progress[training.progress.length - 1]?.total_epochs ?? 0;

  // The model to show in the detail panel: prefer the selected one, fall back
  // to the most recently trained result, otherwise the active model.
  const detailModel: ModelCard | null =
    selectedVersion !== null
      ? (training.models.find((m) => m.version === selectedVersion) ?? null)
      : training.result !== null
        ? (training.models.find(
            (m) => m.version === training.result?.version,
          ) ?? null)
        : (training.models.find((m) => m.is_active) ?? null);

  // Whether the detail panel is showing the live run.
  const showingLiveRun =
    training.isTraining ||
    (training.result !== null &&
      detailModel?.version === training.result.version);

  return (
    <div className="max-w-[1200px] mx-auto px-6 py-10 flex flex-col gap-8">
      <div className="flex items-center justify-between animate-fade-in">
        <div className="flex flex-col gap-1 text-left">
          <h1 className="text-2xl font-semibold tracking-tight">
            Model Training
          </h1>
          <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
            Fine-tune the classifier on your approved transactions
          </p>
        </div>
        <div className="flex items-center gap-2 px-3 py-1.5 text-[10px] font-mono border border-accent/25 bg-accent/5 text-accent uppercase tracking-widest rounded-sm">
          <span>Active Device:</span>
          <span className="font-semibold text-foreground">{device}</span>
        </div>
      </div>

      <MiniLmSetupBanner />

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_2fr] gap-6 animate-fade-in">
        {/* Left column */}
        <div className="flex flex-col gap-6">
          <TrainingControls
            approvedCount={training.approvedCount}
            isTraining={training.isTraining}
            result={training.result}
            error={training.error}
            onStart={(config) => {
              setSelectedVersion(null);
              training.startTraining(config);
            }}
            onStop={training.stopTraining}
          />
          <ModelCardList
            models={training.models}
            isLoading={training.isLoadingModels}
            selectedVersion={selectedVersion}
            onSelect={setSelectedVersion}
            onActivate={training.activateModel}
            onDelete={training.deleteModel}
          />
        </div>

        {/* Right column */}
        <div className="flex flex-col gap-6">
          {showingLiveRun ? (
            <ProgressChart
              history={training.progress}
              totalEpochs={totalEpochs}
              isTraining={training.isTraining}
              embedProgress={training.embedProgress}
            />
          ) : (
            <ModelDetail model={detailModel} />
          )}
          <SamplePredictions
            samples={training.samples}
            isLoading={training.isLoadingSamples}
            isClassifierReady={training.isClassifierReady}
            onRefresh={training.refreshSamples}
          />
        </div>
      </div>
    </div>
  );
}
