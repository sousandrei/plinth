import { invoke } from '@tauri-apps/api/core';
import type { Account } from '@/types';

export const listAccounts = (): Promise<Account[]> =>
  invoke<Account[]>('list_accounts');

export const updateAccount = (
  id: string,
  name: string,
  color: string,
): Promise<Account> => invoke<Account>('update_account', { id, name, color });

export const deleteAccount = (id: string): Promise<void> =>
  invoke<void>('delete_account', { id });
