import { invoke } from '@tauri-apps/api/core';
import type { ExportResult, ImportResult, Space, SpaceMember } from '@/types';

export const listMySpaces = (): Promise<Space[]> =>
  invoke<Space[]>('list_my_spaces');

export const setActiveSpace = (spaceId: string): Promise<void> =>
  invoke<void>('set_active_space', { spaceId });

export const createSpace = (name: string): Promise<Space> =>
  invoke<Space>('create_space', { name });

export const logout = (): Promise<void> => invoke<void>('logout');

export const listSpaceMembers = (): Promise<SpaceMember[]> =>
  invoke<SpaceMember[]>('list_space_members');

export const addSpaceMember = (userId: string): Promise<void> =>
  invoke<void>('add_space_member', { userId });

export const removeSpaceMember = (userId: string): Promise<void> =>
  invoke<void>('remove_space_member', { userId });

export const leaveSpace = (): Promise<void> => invoke<void>('leave_space');

export const deleteSpace = (): Promise<void> => invoke<void>('delete_space');

export const evictSpace = (spaceId: string): Promise<void> =>
  invoke<void>('evict_space', { spaceId });

export const renameSpace = (name: string): Promise<void> =>
  invoke<void>('rename_space', { name });

export const updateMemberRole = (userId: string, role: string): Promise<void> =>
  invoke<void>('update_member_role', { userId, role });

export const exportSpaceData = (
  spaceId: string,
  path: string,
): Promise<ExportResult> =>
  invoke<ExportResult>('export_space_data', { spaceId, path });

export const importSpaceData = (
  spaceId: string,
  path: string,
): Promise<ImportResult> =>
  invoke<ImportResult>('import_space_data', { spaceId, path });
