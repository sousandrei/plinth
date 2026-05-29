import { invoke } from '@tauri-apps/api/core';
import type {
  FinetuneConfig,
  FinetuneProgress,
  FinetuneResult,
  ModelCard,
  TrainingSample,
} from '@/types';

export const fineTune = (config: FinetuneConfig): Promise<FinetuneResult> =>
  invoke<FinetuneResult>('fine_tune', { config });

export const getTrainingProgress = (): Promise<FinetuneProgress[]> =>
  invoke<FinetuneProgress[]>('get_training_progress');

export const listModels = (): Promise<ModelCard[]> =>
  invoke<ModelCard[]>('list_models');

export const setActiveModel = (version: number): Promise<void> =>
  invoke<void>('set_active_model', { version });

export const getTrainingSamples = (
  limit: number,
  version?: number,
): Promise<TrainingSample[]> =>
  invoke<TrainingSample[]>('get_training_samples', { limit, version });

export const countApprovedTransactions = (): Promise<number> =>
  invoke<number>('count_approved_transactions');

export const stopTraining = (): Promise<void> => invoke<void>('stop_training');

export const deleteModel = (version: number): Promise<void> =>
  invoke<void>('delete_model', { version });

export const isClassifierReady = (): Promise<boolean> =>
  invoke<boolean>('is_classifier_ready');

export const minilmStatus = (): Promise<boolean> =>
  invoke<boolean>('minilm_status');

export const ensureMinilm = (): Promise<void> => invoke<void>('ensure_minilm');

export const getTrainingDevice = (): Promise<string> =>
  invoke<string>('get_training_device');
