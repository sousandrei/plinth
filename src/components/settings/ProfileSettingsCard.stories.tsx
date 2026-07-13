import type { Meta } from 'storybook-react-rsbuild';
import type { User } from '@/types';
import { ProfileSettingsCard } from './ProfileSettingsCard';

const user: User = {
  id: '1',
  name: 'Alice',
  has_pin: true,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-03-01T00:00:00Z',
};

const noop = () => {};

const meta = {
  title: 'Settings/ProfileSettingsCard',
  component: ProfileSettingsCard,
  tags: ['autodocs'],
} satisfies Meta<typeof ProfileSettingsCard>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md">
      <ProfileSettingsCard user={user} onSave={noop} isPending={false} />
    </div>
  ),
};

export const Saving = {
  render: () => (
    <div className="w-md">
      <ProfileSettingsCard user={user} onSave={noop} isPending />
    </div>
  ),
};

export const NoUser = {
  render: () => (
    <div className="w-md">
      <ProfileSettingsCard user={null} onSave={noop} isPending={false} />
    </div>
  ),
};
