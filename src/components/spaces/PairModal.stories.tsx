import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { PairToken } from '@/types';
import { PairModal } from './PairModal';

const noop = () => {};

const token: PairToken = {
  token: '482916',
  address: '192.168.1.42',
  expires_at_unix: Math.floor(Date.now() / 1000) + 120,
};

const meta = {
  title: 'Spaces/PairModal',
  component: PairModal,
  tags: ['autodocs'],
} satisfies Meta<typeof PairModal>;

export default meta;

export const BeforeGenerate = {
  render: () => {
    setMock('get_device_name', 'Living Room Mac');
    setMock('get_local_address', '192.168.1.42');
    setMock('generate_pair_token', token);
    return (
      <div className="w-sm">
        <PairModal onClose={noop} />
      </div>
    );
  },
};

export const WithToken = {
  render: () => {
    setMock('get_device_name', 'Living Room Mac');
    setMock('get_local_address', '192.168.1.42');
    return (
      <div className="w-sm">
        <PairModal onClose={noop} initialToken={token} />
      </div>
    );
  },
};

export const ExpiredToken = {
  render: () => {
    setMock('get_device_name', 'Living Room Mac');
    setMock('get_local_address', '192.168.1.42');
    return (
      <div className="w-sm">
        <PairModal
          onClose={noop}
          initialToken={{ ...token, expires_at_unix: 0 }}
        />
      </div>
    );
  },
};
