import { invoke } from '@tauri-apps/api/core';
import type {
  JoinPairing,
  JoinProfile,
  NearbySyncPeer,
  SyncDevice,
  SyncInvitation,
  SyncPairing,
} from '@/types';

export const discoverSyncPeers = (): Promise<NearbySyncPeer[]> =>
  invoke<NearbySyncPeer[]>('discover_sync_peers');

export const getSyncSpaceRevision = (spaceId: string): Promise<number> =>
  invoke<number>('sync_space_revision', { spaceId });

export const createSyncInvitation = (
  spaceId: string,
  sameUser: boolean,
): Promise<SyncInvitation> =>
  invoke<SyncInvitation>('create_sync_invitation', { spaceId, sameUser });

export const beginSyncPairing = ({
  address,
  candidates,
}: {
  address: string;
  candidates?: string[];
}): Promise<JoinPairing> =>
  invoke<JoinPairing>(
    'begin_sync_pairing',
    candidates ? { address, candidates } : { address },
  );

export const verifySyncPairingCode = (
  requestId: string,
  code: string,
): Promise<void> =>
  invoke<void>('verify_sync_pairing_code', { requestId, code });

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
