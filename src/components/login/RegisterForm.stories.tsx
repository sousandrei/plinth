import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { User } from '@/types';
import { RegisterForm } from './RegisterForm';

const noop = () => {};

const user: User = {
  id: '1',
  name: 'Alice',
  has_pin: true,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const setupMocks = () => {
  setMock('create_user', user);
  setMock('set_pin', null);
  setMock('rename_space', null);
};

const meta = {
  title: 'Login/RegisterForm',
  component: RegisterForm,
  tags: ['autodocs'],
} satisfies Meta<typeof RegisterForm>;

export default meta;

export const NameStep = {
  render: () => {
    setupMocks();
    return <RegisterForm onCreated={noop} initialStep="name" />;
  },
};

export const PinStep = {
  render: () => {
    setupMocks();
    return <RegisterForm onCreated={noop} initialStep="pin" />;
  },
};

export const ConfirmStep = {
  render: () => {
    setupMocks();
    return <RegisterForm onCreated={noop} initialStep="confirm" />;
  },
};

export const SpaceStep = {
  render: () => {
    setupMocks();
    return <RegisterForm onCreated={noop} initialStep="space" />;
  },
};

export const WithBack = {
  render: () => {
    setupMocks();
    return <RegisterForm onCreated={noop} onBack={noop} initialStep="name" />;
  },
};
