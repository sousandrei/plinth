import { invoke } from '@tauri-apps/api/core';
import type {
  JoinProfile,
  NearbySyncPeer,
  SyncDevice,
  SyncInvitation,
  SyncPairing,
} from '@/types';

export const discoverSyncPeers = (): Promise<NearbySyncPeer[]> =>
  invoke<NearbySyncPeer[]>('discover_sync_peers');

export const createSyncInvitation = (
  spaceId: string,
  sameUser: boolean,
): Promise<SyncInvitation> =>
  invoke<SyncInvitation>('create_sync_invitation', { spaceId, sameUser });

export const beginSyncPairing = (address: string): Promise<SyncPairing> =>
  invoke<SyncPairing>('begin_sync_pairing', { address });

export const cancelSyncInvitation = (invitationId: string): Promise<void> =>
  invoke<void>('cancel_sync_invitation', { invitationId });

export const getPairingRequest = (
  invitationId: string,
): Promise<SyncPairing | null> =>
  invoke<SyncPairing | null>('sync_invitation_request', { invitationId });

export const respondSyncPairing = (
  requestId: string,
  approved: boolean,
): Promise<void> =>
  invoke<void>('respond_sync_pairing', { requestId, approved });

export const cancelSyncPairing = (requestId: string): Promise<void> =>
  invoke<void>('cancel_sync_pairing', { requestId });

export const joinSharedSpace = (
  requestId: string,
  profile?: JoinProfile,
): Promise<string> =>
  invoke<string>('join_shared_space', { requestId, profile });

export const listSyncDevices = (spaceId: string): Promise<SyncDevice[]> =>
  invoke<SyncDevice[]>('list_sync_devices', { spaceId });

export const removeSyncDevice = (
  spaceId: string,
  deviceId: string,
): Promise<void> => invoke<void>('remove_sync_device', { spaceId, deviceId });
