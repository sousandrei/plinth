import { invoke } from '@tauri-apps/api/core';

export const restartApp = (): Promise<void> => invoke<void>('restart_app');
