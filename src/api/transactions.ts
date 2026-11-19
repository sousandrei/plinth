import { invoke } from '@tauri-apps/api/core';
import type {
  ListTransactionsParams,
  TransactionPage,
  TransactionPatch,
} from '@/types';

export const listTransactions = (
  params: ListTransactionsParams,
): Promise<TransactionPage> =>
  invoke<TransactionPage>('list_transactions', {
    params: {
      page: params.page,
      limit: params.limit,
      search: params.search,
      approved: params.approved,
      category: params.category,
      date_from: params.dateFrom,
      date_to: params.dateTo,
    },
  });

export const updateTransaction = (
  id: string,
  patch: TransactionPatch,
): Promise<void> => invoke<void>('update_transaction', { id, patch });

export const bulkApproveTransactions = (
  ids: string[],
  approved: boolean,
): Promise<number> =>
  invoke<number>('bulk_approve_transactions', { ids, approved });

export const bulkCategorizeTransactions = (
  ids: string[],
  category: string | null,
): Promise<number> =>
  invoke<number>('bulk_categorize_transactions', { ids, category });
