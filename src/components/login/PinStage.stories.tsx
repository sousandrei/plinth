import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { User } from '@/types';
import { PinStage } from './PinStage';

const noop = () => {};

const userWithPin: User = {
  id: '1',
  name: 'Alice',
  has_pin: true,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const userWithoutPin: User = {
  id: '2',
  name: 'Bob',
  has_pin: false,
  created_at: '2025-02-01T00:00:00Z',
  updated_at: '2025-02-01T00:00:00Z',
};

const setupMocks = () => {
  setMock('verify_pin', true);
  setMock('set_pin', null);
};

const meta = {
  title: 'Login/PinStage',
  component: PinStage,
  tags: ['autodocs'],
} satisfies Meta<typeof PinStage>;

export default meta;

export const EnterPin = {
  render: () => {
    setupMocks();
    return <PinStage user={userWithPin} onSuccess={noop} />;
  },
};

export const SetPin = {
  render: () => {
    setupMocks();
    return <PinStage user={userWithoutPin} onSuccess={noop} />;
  },
};

export const WithBack = {
  render: () => {
    setupMocks();
    return <PinStage user={userWithPin} onBack={noop} onSuccess={noop} />;
  },
};
