import { invoke } from '@tauri-apps/api/core';
import type { Aggregations } from '@/types';

export const getAggregations = (userId: string): Promise<Aggregations> =>
  invoke<Aggregations>('get_aggregations', { userId });
