import { invoke } from '@tauri-apps/api/core';

export interface ParserInfo {
  key: string;
  name: string;
  bank: string;
  format: string;
  account_type: string;
  account_source: string;
  is_builtin: boolean;
}

export interface UploadResult {
  inserted: number;
  skipped: number;
  account_name: string;
  logs: string[];
}

export const listParsers = (): Promise<ParserInfo[]> =>
  invoke<ParserInfo[]>('list_parsers');

export const uploadFile = (
  filePath: string,
  parserKey: string,
): Promise<UploadResult> =>
  invoke<UploadResult>('upload_file', {
    filePath,
    parserKey,
  });
