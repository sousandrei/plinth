import { invoke } from '@tauri-apps/api/core';
import type { NearbySyncPeer, SyncDevice, SyncInvitation } from '@/types';

export const discoverSyncPeers = (): Promise<NearbySyncPeer[]> =>
  invoke<NearbySyncPeer[]>('discover_sync_peers');

export const createSyncInvitation = (
  spaceId: string,
): Promise<SyncInvitation> =>
  invoke<SyncInvitation>('create_sync_invitation', { spaceId });

export const joinSharedSpace = (
  code: string,
  address?: string,
): Promise<string> => invoke<string>('join_shared_space', { code, address });

export const listSyncDevices = (spaceId: string): Promise<SyncDevice[]> =>
  invoke<SyncDevice[]>('list_sync_devices', { spaceId });

export const removeSyncDevice = (
  spaceId: string,
  deviceId: string,
): Promise<void> => invoke<void>('remove_sync_device', { spaceId, deviceId });
