import { invoke } from '@tauri-apps/api/core';
import type { Account } from '@/types';

export const listAccounts = (userId: string): Promise<Account[]> =>
  invoke<Account[]>('list_accounts', { userId });

export const updateAccount = (
  id: string,
  name: string,
  color: string,
): Promise<Account> => invoke<Account>('update_account', { id, name, color });
