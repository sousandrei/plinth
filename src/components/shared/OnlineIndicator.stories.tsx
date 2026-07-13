import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { PeerInfo } from '@/types';
import { OnlineIndicator } from './OnlineIndicator';

const meta = {
  title: 'Shared/OnlineIndicator',
  component: OnlineIndicator,
} satisfies Meta<typeof OnlineIndicator>;

export default meta;

const makePeers = (offsets: number[]): PeerInfo[] => {
  const now = Math.floor(Date.now() / 1000);
  return offsets.map((offset, i) => ({
    device_id: `dev-${i + 1}`,
    name: i === 0 ? 'MacBook Pro' : i === 1 ? 'iPhone 15' : `Device ${i + 1}`,
    host: `192.168.1.${10 + i}`,
    port: 8420,
    last_seen: now + offset,
  }));
};

export const Active = {
  render: () => {
    setMock('list_peers', () => makePeers([0, -2]));
    return (
      <div className="flex items-center h-14 bg-canvas-raised px-4">
        <OnlineIndicator />
      </div>
    );
  },
};

export const Inactive = {
  render: () => {
    setMock('list_peers', () => makePeers([-10]));
    return (
      <div className="flex items-center h-14 bg-canvas-raised px-4">
        <OnlineIndicator />
      </div>
    );
  },
};

export const NoPeers = {
  render: () => {
    setMock('list_peers', []);
    return (
      <div className="flex items-center h-14 bg-canvas-raised px-4">
        <OnlineIndicator />
      </div>
    );
  },
};
