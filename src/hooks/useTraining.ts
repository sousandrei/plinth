import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';
import {
  isClassifierReady as checkClassifierReady,
  countApprovedTransactions,
  deleteModel,
  fineTune,
  getTrainingProgress,
  getTrainingSamples,
  listModels,
  setActiveModel,
  stopTraining,
} from '@/api/training';
import { getDemoModels } from '@/demo/generators';
import { useDemoMode } from '@/hooks/useDemoMode';
import type {
  FinetuneConfig,
  FinetuneProgress,
  FinetuneResult,
  ModelCard,
  TrainingSample,
} from '@/types';

const MODELS_KEY = ['models'] as const;
const PROGRESS_KEY = ['training-progress'] as const;
const APPROVED_COUNT_KEY = ['approved-count'] as const;
const SAMPLES_KEY_PREFIX = ['training-samples'] as const;
const SAMPLES_KEY = (limit: number, version: number | null) =>
  ['training-samples', limit, version] as const;

const DEFAULT_CONFIG: FinetuneConfig = {
  epochs: 10,
  batch_size: 32,
  learning_rate: 1e-3,
};

const SAMPLES_LIMIT = 50;

export interface TrainingState {
  // Data
  models: ModelCard[];
  samples: TrainingSample[];
  approvedCount: number;
  progress: FinetuneProgress[];
  result: FinetuneResult | null;

  // Status
  isClassifierReady: boolean;
  isLoadingModels: boolean;
  isLoadingSamples: boolean;
  isTraining: boolean;
  error: string | null;

  // Actions
  startTraining: (config?: Partial<FinetuneConfig>) => void;
  stopTraining: () => void;
  activateModel: (version: number) => void;
  deleteModel: (version: number) => void;
  refreshSamples: () => void;
}

export function useTraining(
  selectedVersion: number | null = null,
): TrainingState {
  const queryClient = useQueryClient();
  const { isDemoMode } = useDemoMode();
  const [isClassifierReady, setIsClassifierReady] = useState(false);

  // Check immediately in case the classifier finished loading before this
  // component mounted (event would have been missed).
  useEffect(() => {
    checkClassifierReady().then((ready) => {
      if (ready) setIsClassifierReady(true);
    });
  }, []);

  // Also listen for the event for the case where we mount before loading finishes.
  // The same effect also subscribes to training progress/done events.
  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    listen<void>('classifier://ready', () => {
      if (!cancelled) {
        setIsClassifierReady(true);
        queryClient.invalidateQueries({ queryKey: SAMPLES_KEY_PREFIX });
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    listen<void>('training://progress', () => {
      if (!cancelled) queryClient.invalidateQueries({ queryKey: PROGRESS_KEY });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    listen<void>('training://done', () => {
      if (!cancelled) queryClient.invalidateQueries({ queryKey: MODELS_KEY });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    return () => {
      cancelled = true;
      for (const fn of unlisteners) fn();
    };
  }, [queryClient]);

  const modelsQuery = useQuery({
    queryKey: [...MODELS_KEY, isDemoMode ? 'demo' : 'real'],
    queryFn: isDemoMode ? getDemoModels : listModels,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  const progressQuery = useQuery({
    queryKey: PROGRESS_KEY,
    queryFn: getTrainingProgress,
    initialData: [],
  });

  const samplesQuery = useQuery({
    queryKey: SAMPLES_KEY(SAMPLES_LIMIT, selectedVersion),
    queryFn: () =>
      getTrainingSamples(SAMPLES_LIMIT, selectedVersion ?? undefined),
    enabled: isClassifierReady,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  const approvedCountQuery = useQuery({
    queryKey: APPROVED_COUNT_KEY,
    queryFn: countApprovedTransactions,
    enabled: true,
  });

  const trainMutation = useMutation({
    mutationFn: (config: FinetuneConfig) => fineTune(config),
    onMutate: () => {
      queryClient.setQueryData(PROGRESS_KEY, []);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: MODELS_KEY });
      queryClient.invalidateQueries({ queryKey: PROGRESS_KEY });
    },
  });

  const stopMutation = useMutation({
    mutationFn: stopTraining,
  });

  const activateMutation = useMutation({
    mutationFn: (version: number) => setActiveModel(version),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: MODELS_KEY });
      queryClient.invalidateQueries({ queryKey: SAMPLES_KEY_PREFIX });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (version: number) => deleteModel(version),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: MODELS_KEY });
      queryClient.invalidateQueries({ queryKey: SAMPLES_KEY_PREFIX });
    },
  });

  const handleStartTraining = useCallback(
    (config?: Partial<FinetuneConfig>) => {
      trainMutation.mutate({ ...DEFAULT_CONFIG, ...config });
    },
    [trainMutation],
  );

  const handleStopTraining = useCallback(() => {
    stopMutation.mutate();
  }, [stopMutation]);

  const handleActivateModel = useCallback(
    (version: number) => {
      activateMutation.mutate(version);
    },
    [activateMutation],
  );

  const handleDeleteModel = useCallback(
    (version: number) => {
      deleteMutation.mutate(version);
    },
    [deleteMutation],
  );

  const handleRefreshSamples = useCallback(() => {
    queryClient.invalidateQueries({
      queryKey: SAMPLES_KEY(SAMPLES_LIMIT, selectedVersion),
    });
  }, [queryClient, selectedVersion]);

  const mutationError =
    trainMutation.error ?? activateMutation.error ?? deleteMutation.error;
  const error =
    mutationError instanceof Error
      ? mutationError.message
      : mutationError
        ? String(mutationError)
        : null;

  return {
    models: modelsQuery.data ?? [],
    samples: samplesQuery.data ?? [],
    approvedCount: approvedCountQuery.data ?? 0,
    progress: progressQuery.data,
    result: trainMutation.data ?? null,
    isClassifierReady,
    isLoadingModels: modelsQuery.isLoading,
    isLoadingSamples: samplesQuery.isLoading || !isClassifierReady,
    isTraining: trainMutation.isPending,
    error,
    startTraining: handleStartTraining,
    stopTraining: handleStopTraining,
    activateModel: handleActivateModel,
    deleteModel: handleDeleteModel,
    refreshSamples: handleRefreshSamples,
  };
}
