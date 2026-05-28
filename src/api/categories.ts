import { invoke } from '@tauri-apps/api/core';
import type { Category } from '@/types';

export const listAllCategories = (): Promise<Category[]> =>
  invoke<Category[]>('list_all_categories');

export const createCategory = (
  name: string,
  color: string,
): Promise<Category> => invoke<Category>('create_category', { name, color });

export const deleteCategory = (id: string): Promise<void> =>
  invoke<void>('delete_category', { id });

export const updateCategory = (
  id: string,
  name: string,
  color: string,
): Promise<Category> =>
  invoke<Category>('update_category', { id, name, color });
