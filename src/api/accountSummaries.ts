import { invoke } from '@tauri-apps/api/core';
import type { AccountSummary } from '@/types';

export interface AccountSummaryPage {
  rows: AccountSummary[];
  page_count: number;
}

export const listAccountSummaries = (
  userId: string,
  page: number,
  limit: number,
): Promise<AccountSummaryPage> =>
  invoke<AccountSummaryPage>('list_account_summaries', { userId, page, limit });

export const upsertAccountSummary = (
  month: string,
  accountId: string,
  balance: number,
): Promise<void> =>
  invoke<void>('upsert_account_summary', {
    month,
    accountId: accountId,
    balance,
  });

export const deleteAccountSummary = (
  month: string,
  accountId: string,
): Promise<void> =>
  invoke<void>('delete_account_summary', { month, accountId: accountId });
