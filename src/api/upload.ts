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
  account_id: string;
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

export interface TransactionInput {
  text: string;
  amount: number;
  booking_date: string;
}

/// Predict categories for one or more transactions using the locally
/// loaded finetuned model. Returns one prediction per input in the same
/// order. An empty string means the classifier had no opinion.
export const classifyTransactions = (
  transactions: TransactionInput[],
): Promise<string[]> =>
  invoke<string[]>('classify_transactions', { transactions });
