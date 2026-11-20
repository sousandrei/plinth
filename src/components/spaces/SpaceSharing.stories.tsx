import { expect, fn, waitFor, within } from 'storybook/test';
import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { LoginPage } from '@/components/login/LoginPage';
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
const code = '032762';
const createInvitation = fn(() => ({
  id: 'join-household',
  name: 'Living room PC',
  port: 37420,
  addresses: ['192.168.1.20:37420'],
  nearby_available: true,
}));
const beginPairing = fn(() => ({
  id: 'outgoing',
  device: 'Living room PC',
  same_user: false,
}));
const verifyCode = fn(({ code: entered }: { code: string }) => {
  if (entered !== code) throw new Error('Incorrect joining code.');
});
const respond = fn();
const joinSpace = fn().mockName('join_shared_space');
const activateSpace = fn().mockName('set_active_space');

const setupSpaces = () => {
  const spaces = [household, memberSpace];
  const session = {
    user: { ...household, id: 'user', has_pin: false },
    space_id: household.id,
  };
  setMock('list_my_spaces', () => [...spaces]);
  setMock('get_session', () => ({ ...session }));
  activateSpace.mockReset().mockImplementation(({ spaceId }) => {
    session.space_id = spaceId;
  });
  return spaces;
};

const meta = {
  title: 'Spaces/Sharing',
  component: SpacesPage,
  beforeEach: () => {
    verifyCode.mockClear();
    setMock('verify_sync_pairing_code', verifyCode);
  },
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
    setMock('create_sync_invitation', createInvitation);
    setMock('sync_invitation_request', null);
    setMock('respond_sync_pairing', respond);
    setMock('begin_sync_pairing', beginPairing);
    setMock('join_shared_space', joinSpace);
    setMock('set_active_space', activateSpace);
    setMock('discover_sync_peers', [
      {
        name: 'join-household',
        display_name: 'Living room PC',
        addresses: ['192.168.1.20:37420'],
      },
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
      await body.findByRole('button', { name: 'Pair my device' }),
    );
    await expect(await body.findByText('192.168.1.20:37420')).toBeVisible();
    setMock('sync_invitation_request', {
      id: 'incoming',
      device: 'MacBook',
      code,
      same_user: true,
    });
    await expect(
      await body.findByText(code, {}, { timeout: 3000 }),
    ).toBeVisible();
    await expect(createInvitation).toHaveBeenCalledWith({
      spaceId: household.id,
      sameUser: true,
    });
    await userEvent.click(body.getByRole('button', { name: 'Approve' }));
    await expect(
      await body.findByText('Approved. Finish joining on the other device.'),
    ).toBeVisible();
    await expect(respond).toHaveBeenCalledWith({
      requestId: 'incoming',
      approved: true,
    });
    await userEvent.keyboard('{Escape}');
    await userEvent.click(
      canvas.getByRole('button', { name: 'Join shared space' }),
    );
    await userEvent.click(
      await body.findByRole('button', { name: 'Living room PC' }),
    );
    const firstDigit = await body.findByRole('textbox', { name: 'Digit 1' });
    await expect(body.queryByText(code)).not.toBeInTheDocument();
    await userEvent.click(firstDigit);
    await userEvent.paste('000000');
    await expect(
      await body.findByText('Error: Incorrect joining code.'),
    ).toBeVisible();
    await expect(
      body.queryByRole('button', { name: 'Join space' }),
    ).not.toBeInTheDocument();
    await userEvent.click(firstDigit);
    await userEvent.paste(code);
    await expect(await body.findByText(/Code accepted/)).toBeVisible();
    await expect(verifyCode).toHaveBeenCalledWith({
      requestId: 'outgoing',
      code,
    });
    await expect(beginPairing).toHaveBeenCalledWith({
      address: 'mdns:join-household',
    });
    await userEvent.click(body.getByRole('button', { name: 'Join space' }));
    await waitFor(() =>
      expect(body.getByText('Shared space joined')).toBeVisible(),
    );
    await expect(joinSpace).toHaveBeenCalledWith({
      requestId: 'outgoing',
      profile: undefined,
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
    beginPairing.mockClear();
    setMock('begin_sync_pairing', beginPairing);
    setMock('discover_sync_peers', () => {
      throw new Error('unavailable');
    });
  },
  play: async ({ canvas, canvasElement, userEvent }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(
      await canvas.findByRole('button', { name: 'Join shared space' }),
    );
    await expect(body.getByLabelText('Host or IP address')).not.toBeVisible();
    await userEvent.click(body.getByText('Join by address'));
    await expect(body.getByLabelText('Host or IP address')).toBeVisible();
    await expect(
      await body.findByText('Nearby discovery is unavailable.'),
    ).toBeInTheDocument();
    await userEvent.type(
      body.getByLabelText('Host or IP address'),
      '192.168.1.20:37420',
    );
    await userEvent.click(body.getByRole('button', { name: 'Connect' }));
    await expect(
      await body.findByRole('textbox', { name: 'Digit 1' }),
    ).toBeVisible();
    await expect(beginPairing).toHaveBeenCalledWith({
      address: '192.168.1.20:37420',
    });
  },
};

export const JoinFromWelcome: Story = {
  render: () => (
    <AuthProvider>
      <LoginPage />
      <Toaster />
    </AuthProvider>
  ),
  beforeEach: () => {
    setMock('list_users', []);
    setMock('get_session', { user: null, space_id: null });
    setMock('discover_sync_peers', [
      {
        name: 'join-household',
        display_name: 'Living room PC',
        addresses: ['192.168.1.20:37420'],
      },
    ]);
    setMock('begin_sync_pairing', {
      id: 'outgoing',
      device: 'Living room PC',
      same_user: true,
    });
    joinSpace.mockReset().mockReturnValue(joined.id);
    setMock('join_shared_space', joinSpace);
    setMock('set_active_space', activateSpace);
  },
  play: async ({ canvas, canvasElement, userEvent }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(
      await canvas.findByRole('button', { name: 'Join shared space' }),
    );
    await userEvent.click(
      await body.findByRole('button', { name: 'Living room PC' }),
    );
    await userEvent.click(
      await body.findByRole('textbox', { name: 'Digit 1' }),
    );
    await userEvent.paste(code);
    await expect(await body.findByText(/Code accepted/)).toBeVisible();
    await expect(body.queryByLabelText('Your name')).not.toBeInTheDocument();
    await userEvent.type(body.getByLabelText('PIN for this device'), '1234');
    await userEvent.type(body.getByLabelText('Confirm PIN'), '1234');
    await userEvent.click(body.getByRole('button', { name: 'Join space' }));
    await waitFor(() =>
      expect(joinSpace).toHaveBeenCalledWith({
        requestId: 'outgoing',
        profile: { pin: '1234', name: undefined },
      }),
    );
  },
};
