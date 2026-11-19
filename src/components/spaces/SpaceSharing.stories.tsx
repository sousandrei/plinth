import { expect, fn, waitFor, within } from 'storybook/test';
import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { Toaster } from '@/components/ui/Toast';
import { AuthProvider } from '@/context/AuthContext';
import { setMock } from '@/lib/tauri-mock';
import { SpacesPage } from '@/routes/spaces';
import type { Space } from '@/types';

const household: Space = {
  id: 'household',
  name: 'Household',
  role: 'owner',
  created_at: '2026-11-19',
  updated_at: '2026-11-19',
};
const memberSpace: Space = {
  ...household,
  id: 'member-space',
  name: 'Member Space',
  role: 'member',
};
const joined: Space = { ...household, id: 'joined', name: 'Joined Space' };
const code = 'plinth1.invitation-for-the-story';
const createInvitation = fn(() => ({
  code,
  name: 'join-household',
  port: 37420,
}));
const joinSpace = fn().mockName('join_shared_space');
const activateSpace = fn().mockName('set_active_space');

const setupSpaces = () => {
  const spaces = [household, memberSpace];
  setMock('list_my_spaces', () => [...spaces]);
  setMock('get_session', {
    user: { ...household, id: 'user', has_pin: false },
    space_id: household.id,
  });
  return spaces;
};

const meta = {
  title: 'Spaces/Sharing',
  component: SpacesPage,
  render: () => (
    <AuthProvider>
      <SpacesPage />
      <Toaster />
    </AuthProvider>
  ),
} satisfies Meta<typeof SpacesPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ShareAndJoin: Story = {
  beforeEach: () => {
    const spaces = setupSpaces();
    createInvitation.mockClear();
    joinSpace.mockReset().mockImplementation(() => {
      spaces.push(joined);
      return joined.id;
    });
    activateSpace.mockClear();
    setMock('create_sync_invitation', createInvitation);
    setMock('join_shared_space', joinSpace);
    setMock('set_active_space', activateSpace);
    setMock('discover_sync_peers', [
      { name: 'join-household', addresses: ['192.168.1.20:37420'] },
    ]);
  },
  play: async ({ canvas, canvasElement, userEvent }) => {
    const body = within(canvasElement.ownerDocument.body);
    const share = await canvas.findByRole('button', {
      name: 'Share space: Household',
    });
    await expect(
      canvas.queryByRole('button', { name: 'Share space: Member Space' }),
    ).not.toBeInTheDocument();
    await userEvent.click(share);
    await userEvent.click(
      await body.findByRole('button', { name: 'Create invitation' }),
    );
    await expect(await body.findByLabelText('Invitation code')).toHaveValue(
      code,
    );
    await expect(createInvitation).toHaveBeenCalledWith({
      spaceId: household.id,
    });
    await userEvent.keyboard('{Escape}');
    await userEvent.click(
      canvas.getByRole('button', { name: 'Join shared space' }),
    );
    await userEvent.click(
      await body.findByRole('button', { name: 'join-household' }),
    );
    await userEvent.type(body.getByLabelText('Invitation code'), code);
    await userEvent.click(body.getByRole('button', { name: /^Join$/ }));
    await waitFor(() =>
      expect(body.getByText('Shared space joined')).toBeVisible(),
    );
    await expect(joinSpace).toHaveBeenCalledWith({
      code,
      address: 'mdns:join-household',
    });
    await expect(activateSpace).toHaveBeenCalledWith({ spaceId: joined.id });
    await expect(
      (await canvas.findByText('Joined Space')).closest(
        '[aria-current="true"]',
      ),
    ).toBeInTheDocument();
  },
};

export const DiscoveryUnavailable: Story = {
  beforeEach: () => {
    setupSpaces();
    setMock('discover_sync_peers', () => {
      throw new Error('unavailable');
    });
  },
  play: async ({ canvas, canvasElement, userEvent }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(
      await canvas.findByRole('button', { name: 'Join shared space' }),
    );
    await expect(
      await body.findByLabelText('Host or IP address (optional)'),
    ).toBeVisible();
    await expect(
      await body.findByText(
        'Discovery unavailable. You can enter an address below.',
      ),
    ).toBeInTheDocument();
  },
};
