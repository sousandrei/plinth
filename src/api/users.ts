import { invoke } from '@tauri-apps/api/core';
import type { User } from '@/types';

export const listUsers = (): Promise<User[]> => invoke<User[]>('list_users');

export const createUser = (name: string): Promise<User> =>
  invoke<User>('create_user', { name });

export const setPin = (userId: string, pin: string): Promise<void> =>
  invoke<void>('set_pin', { userId, pin });

export const verifyPin = (userId: string, pin: string): Promise<boolean> =>
  invoke<boolean>('verify_pin', { userId, pin });

export const updateUserName = (userId: string, name: string): Promise<User> =>
  invoke<User>('update_user_name', { userId, name });

export const factoryReset = (): Promise<void> => invoke<void>('factory_reset');
