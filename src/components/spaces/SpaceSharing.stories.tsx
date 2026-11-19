import { expect, fn, waitFor, within } from 'storybook/test';
import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { Toaster } from '@/components/ui/Toast';
import { AuthProvider } from '@/context/AuthContext';
import { setMock } from '@/lib/tauri-mock';
import {
  setIdentifierMock,
  setOpenMock,
  setSaveMock,
} from '@/lib/tauri-plugin-mocks';
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
const seedPath = '/tmp/household-space-seed.json';
const exportSeed = fn().mockName('export_sync_seed');
const importSeed = fn().mockName('import_sync_seed');
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
    setIdentifierMock('plinth-sync-preview');
    setSaveMock(async () => seedPath);
    setOpenMock(async () => seedPath);
    exportSeed.mockClear();
    importSeed.mockReset().mockImplementation(() => {
      spaces.push(joined);
      return joined.id;
    });
    activateSpace.mockClear();
    setMock('export_sync_seed', exportSeed);
    setMock('import_sync_seed', importSeed);
    setMock('set_active_space', activateSpace);
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
    await waitFor(() =>
      expect(body.getByText('Space seed saved')).toBeVisible(),
    );
    await expect(exportSeed).toHaveBeenCalledWith({
      spaceId: household.id,
      path: seedPath,
    });
    await userEvent.click(
      canvas.getByRole('button', { name: 'Join shared space' }),
    );
    await waitFor(() =>
      expect(body.getByText('Shared space joined')).toBeVisible(),
    );
    await expect(importSeed).toHaveBeenCalledWith({ path: seedPath });
    await expect(activateSpace).toHaveBeenCalledWith({ spaceId: joined.id });
    await expect(
      (await canvas.findByText('Joined Space')).closest(
        '[aria-current="true"]',
      ),
    ).toBeInTheDocument();
  },
};

export const RegularProfile: Story = {
  beforeEach: () => {
    setupSpaces();
  },
  play: async ({ canvas }) => {
    await canvas.findByText('Household');
    await expect(
      canvas.queryByRole('button', { name: 'Join shared space' }),
    ).not.toBeInTheDocument();
    await expect(
      canvas.queryByRole('button', { name: 'Share space: Household' }),
    ).not.toBeInTheDocument();
  },
};
