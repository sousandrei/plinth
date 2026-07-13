import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { TrustedDevice } from '@/types';
import { DevicesSection } from './DevicesSection';

const devices: TrustedDevice[] = [
  {
    id: '1',
    space_id: 'space-1',
    device_id: 'dev-local',
    display_name: 'Living Room Mac',
    paired_at: '2025-01-15T10:30:00Z',
  },
  {
    id: '2',
    space_id: 'space-1',
    device_id: 'dev-remote',
    display_name: 'Office PC',
    paired_at: '2025-02-20T14:00:00Z',
  },
];

const meta = {
  title: 'Spaces/DevicesSection',
  component: DevicesSection,
} satisfies Meta<typeof DevicesSection>;

export default meta;

export const WithDevices = {
  render: () => {
    setMock('get_local_device_id', 'dev-local');
    setMock('list_trusted_devices', devices);
    setMock('remove_trusted_device', null);
    setMock('get_device_name', 'Storybook');
    setMock('get_local_address', '127.0.0.1');
    return (
      <div className="w-md">
        <DevicesSection spaceId="space-1" />
      </div>
    );
  },
};

export const NoDevices = {
  render: () => {
    setMock('get_local_device_id', 'dev-local');
    setMock('list_trusted_devices', []);
    setMock('get_device_name', 'Storybook');
    setMock('get_local_address', '127.0.0.1');
    return (
      <div className="w-md">
        <DevicesSection spaceId="space-1" />
      </div>
    );
  },
};
