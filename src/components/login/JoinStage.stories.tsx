import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { PeerInfo } from '@/types';
import { JoinStage } from './JoinStage';

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
  title: 'Login/JoinStage',
  component: JoinStage,
} satisfies Meta<typeof JoinStage>;

export default meta;

export const WithPeers = {
  render: () => {
    setMock('get_device_name', 'Storybook Device');
    setMock('list_peers', peers);
    setMock('join_space', {
      space_id: 'space-1',
      space_name: 'Shared',
      users: [
        { id: 'u1', name: 'Alice' },
        { id: 'u2', name: 'Bob' },
      ],
    });
    setMock('set_pin', null);
    setMock('verify_pin', true);
    setMock('create_user_in_space', {
      id: 'u3',
      name: 'New',
      has_pin: false,
      created_at: '',
      updated_at: '',
    });
    setMock('set_active_space', null);
    return <JoinStage onSuccess={noop} onBack={noop} />;
  },
};

export const NoPeers = {
  render: () => {
    setMock('get_device_name', 'Storybook Device');
    setMock('list_peers', []);
    return <JoinStage onSuccess={noop} onBack={noop} />;
  },
};
