import { invoke } from '@tauri-apps/api/core';
import type { ListTransactionsParams, TransactionPage } from '@/types';

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
  approved: boolean,
  note: string,
  category: string | null,
): Promise<void> =>
  invoke<void>('update_transaction', { id, approved, note, category });
