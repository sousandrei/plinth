import type { Meta } from 'storybook-react-rsbuild';
import type { User } from '@/types';
import { UserSelect } from './UserSelect';

const noop = () => {};

const users: User[] = [
  {
    id: '1',
    name: 'Alice',
    has_pin: true,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-03-01T00:00:00Z',
  },
  {
    id: '2',
    name: 'Bob',
    has_pin: true,
    created_at: '2025-02-15T00:00:00Z',
    updated_at: '2025-03-01T00:00:00Z',
  },
];

const meta = {
  title: 'Login/UserSelect',
  component: UserSelect,
  tags: ['autodocs'],
} satisfies Meta<typeof UserSelect>;

export default meta;

export const Default = {
  render: () => (
    <UserSelect users={users} onSelect={noop} onAdd={noop} onJoin={noop} />
  ),
};

export const SingleUser = {
  render: () => (
    <UserSelect users={[users[0]]} onSelect={noop} onAdd={noop} onJoin={noop} />
  ),
};

export const NoUsers = {
  render: () => (
    <UserSelect users={[]} onSelect={noop} onAdd={noop} onJoin={noop} />
  ),
};
