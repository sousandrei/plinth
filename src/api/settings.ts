import { invoke } from '@tauri-apps/api/core';

export const getAppSetting = (key: string): Promise<string | null> =>
  invoke<string | null>('get_app_setting', { key });

export const setAppSetting = (key: string, value: string): Promise<void> =>
  invoke<void>('set_app_setting', { key, value });
