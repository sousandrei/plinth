import type { Update } from '@tauri-apps/plugin-updater';
import type { Meta } from 'storybook-react-rsbuild';
import { setCheckMock } from '@/lib/tauri-plugin-mocks';
import { UpdateSettingsCard } from './UpdateSettingsCard';

const updateAvailable = {
  version: '0.2.0',
  date: '2025-03-15',
  body: '## New Features\n- Dark mode support\n- Transaction search\n\n## Fixes\n- Fixed CSV import edge case',
  downloadAndInstall: () => Promise.resolve(),
} as unknown as Update;

const meta = {
  title: 'Settings/UpdateSettingsCard',
  component: UpdateSettingsCard,
  tags: ['autodocs'],
} satisfies Meta<typeof UpdateSettingsCard>;

export default meta;

export const Idle = {
  render: () => {
    setCheckMock(() => Promise.resolve(null));
    return (
      <div className="w-md">
        <UpdateSettingsCard />
      </div>
    );
  },
};

export const Checking = {
  render: () => (
    <div className="w-md">
      <UpdateSettingsCard initialStatus="checking" />
    </div>
  ),
};

export const UpdateAvailable = {
  render: () => {
    setCheckMock(() => Promise.resolve(updateAvailable));
    return (
      <div className="w-md">
        <UpdateSettingsCard
          initialStatus="available"
          initialUpdateInfo={updateAvailable}
        />
      </div>
    );
  },
};

export const Downloading = {
  render: () => (
    <div className="w-md">
      <UpdateSettingsCard
        initialStatus="downloading"
        initialUpdateInfo={updateAvailable}
        initialDownloadedBytes={1572864}
        initialTotalBytes={5242880}
      />
    </div>
  ),
};

export const Installed = {
  render: () => (
    <div className="w-md">
      <UpdateSettingsCard
        initialStatus="installed"
        initialUpdateInfo={updateAvailable}
      />
    </div>
  ),
};
