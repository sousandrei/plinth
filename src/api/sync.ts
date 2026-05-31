import { invoke } from '@tauri-apps/api/core';
import type { JoinResult, PairToken, PeerInfo, TrustedDevice } from '@/types';

export const listPeers = (): Promise<PeerInfo[]> =>
  invoke<PeerInfo[]>('list_peers');

export const getDeviceName = (): Promise<string> =>
  invoke<string>('get_device_name');

export const listTrustedDevices = (): Promise<TrustedDevice[]> =>
  invoke<TrustedDevice[]>('list_trusted_devices');

export const removeTrustedDevice = (id: string): Promise<void> =>
  invoke<void>('remove_trusted_device', { id });

export const setTrustedDeviceSync = (
  id: string,
  enabled: boolean,
): Promise<void> => invoke<void>('set_trusted_device_sync', { id, enabled });

export const generatePairToken = (
  hostDisplayName: string,
): Promise<PairToken> =>
  invoke<PairToken>('generate_pair_token', { hostDisplayName });

export const acceptPairToken = (
  address: string,
  deviceDisplayName: string,
): Promise<JoinResult> =>
  invoke<JoinResult>('accept_pair_token', { address, deviceDisplayName });

export interface SpaceUsers {
  space_id: string;
  space_name: string;
  users: Array<{ id: string; name: string }>;
}

export const joinSpace = (
  peerDeviceId: string,
  token: string,
  deviceDisplayName: string,
): Promise<SpaceUsers> =>
  invoke<SpaceUsers>('join_space', { peerDeviceId, token, deviceDisplayName });

export const acceptPairTokenFromPeer = (
  peerDeviceId: string,
  token: string,
  deviceDisplayName: string,
): Promise<JoinResult> =>
  invoke<JoinResult>('accept_pair_token_from_peer', {
    peerDeviceId,
    token,
    deviceDisplayName,
  });
