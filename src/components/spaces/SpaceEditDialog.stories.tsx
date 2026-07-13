import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { Space, SpaceMember, User } from '@/types';
import { SpaceEditDialog } from './SpaceEditDialog';

const noop = () => {};

const space: Space = {
  id: 'space-1',
  name: 'Personal',
  role: 'owner',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-03-01T00:00:00Z',
};

const members: SpaceMember[] = [
  { user_id: 'u1', name: 'Alice', role: 'owner' },
  { user_id: 'u2', name: 'Bob', role: 'member' },
];

const allUsers: User[] = [
  { id: 'u1', name: 'Alice', has_pin: true, created_at: '', updated_at: '' },
  { id: 'u2', name: 'Bob', has_pin: true, created_at: '', updated_at: '' },
  { id: 'u3', name: 'Charlie', has_pin: false, created_at: '', updated_at: '' },
];

const setupMocks = () => {
  setMock('list_space_members', members);
  setMock('list_users', allUsers);
  setMock('rename_space', null);
  setMock('update_member_role', null);
  setMock('remove_space_member', null);
  setMock('remove_user', null);
  setMock('add_space_member', null);
  setMock('add_app_user', {
    id: 'u4',
    name: 'New User',
    has_pin: false,
    created_at: '',
    updated_at: '',
  });
  setMock('delete_space', null);
  setMock('leave_space', null);
  setMock('force_sync_now', { dialed: 0, ok: 0, failed: [] });
  setMock('list_trusted_devices', []);
  setMock('get_local_device_id', 'dev-local');
  setMock('get_device_name', 'Storybook');
  setMock('get_local_address', '127.0.0.1');
  setMock('generate_pair_token', {
    token: '482916',
    address: '127.0.0.1',
    expires_at_unix: Math.floor(Date.now() / 1000) + 120,
  });
};

const meta = {
  title: 'Spaces/SpaceEditDialog',
  component: SpaceEditDialog,
} satisfies Meta<typeof SpaceEditDialog>;

export default meta;

export const Owner = {
  render: () => {
    setupMocks();
    return (
      <div className="w-lg">
        <SpaceEditDialog
          space={space}
          currentUserId="u1"
          onClose={noop}
          onDeleted={noop}
        />
      </div>
    );
  },
};

export const Member = {
  render: () => {
    setupMocks();
    return (
      <div className="w-lg">
        <SpaceEditDialog
          space={{ ...space, role: 'member' }}
          currentUserId="u2"
          onClose={noop}
          onDeleted={noop}
        />
      </div>
    );
  },
};
