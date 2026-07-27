import { invoke } from '@tauri-apps/api/core';
import type { ParserInfo } from './upload';

export interface ParserFileInfo {
  filename: string;
  path: string;
  is_builtin: boolean;
  content: string;
  units: ParserInfo[];
}

export const listParserFiles = (): Promise<ParserFileInfo[]> =>
  invoke<ParserFileInfo[]>('list_parser_files');

export const saveParserFile = (path: string, code: string): Promise<void> =>
  invoke<void>('save_parser_file', { path, code });

export const createParserFile = (
  filename: string,
  code: string,
): Promise<ParserFileInfo> =>
  invoke<ParserFileInfo>('create_parser_file', { filename, code });

export const deleteParserFile = (path: string): Promise<void> =>
  invoke<void>('delete_parser_file', { path });

export const extractUnitsFromCode = (code: string): Promise<ParserInfo[]> =>
  invoke<ParserInfo[]>('extract_units_from_code', { code });

export interface TestTransformResult {
  result: string;
  logs: string[];
}

export const testParserTransform = (
  filePath: string,
  scriptCode: string,
  unitKey: string,
): Promise<TestTransformResult> =>
  invoke<TestTransformResult>('test_parser_transform', {
    filePath,
    scriptCode,
    unitKey,
  });

export interface ClassificationInput {
  text: string;
  amount: number;
  booking_date: string;
}

export const classifyTransactions = (
  transactions: ClassificationInput[],
): Promise<string[]> =>
  invoke<string[]>('classify_transactions', { transactions });
