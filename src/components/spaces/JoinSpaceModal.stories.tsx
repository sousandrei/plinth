import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { PeerInfo } from '@/types';
import { JoinSpaceModal } from './JoinSpaceModal';

const noop = () => {};

const peers: PeerInfo[] = [
  {
    device_id: 'dev-1',
    name: 'Living Room Mac',
    host: '192.168.1.42',
    port: 5037,
    last_seen: Date.now(),
  },
  {
    device_id: 'dev-2',
    name: 'Office PC',
    host: '192.168.1.99',
    port: 5037,
    last_seen: Date.now(),
  },
];

const meta = {
  title: 'Spaces/JoinSpaceModal',
  component: JoinSpaceModal,
} satisfies Meta<typeof JoinSpaceModal>;

export default meta;

export const WithPeers = {
  render: () => {
    setMock('get_device_name', 'Storybook Device');
    setMock('list_peers', peers);
    setMock('accept_pair_token_from_peer', {
      space_id: 'space-1',
      space_name: 'Shared',
    });
    return (
      <div className="w-sm">
        <JoinSpaceModal onClose={noop} onJoined={noop} />
      </div>
    );
  },
};

export const NoPeers = {
  render: () => {
    setMock('get_device_name', 'Storybook Device');
    setMock('list_peers', []);
    return (
      <div className="w-sm">
        <JoinSpaceModal onClose={noop} onJoined={noop} />
      </div>
    );
  },
};
