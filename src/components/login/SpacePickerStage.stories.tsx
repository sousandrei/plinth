import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { Space } from '@/types';
import { SpacePickerStage } from './SpacePickerStage';

const spaces: Space[] = [
  {
    id: 'space-1',
    name: 'Personal',
    role: 'owner',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-03-01T00:00:00Z',
  },
  {
    id: 'space-2',
    name: 'Shared Finances',
    role: 'member',
    created_at: '2025-02-15T00:00:00Z',
    updated_at: '2025-03-01T00:00:00Z',
  },
];

const noop = () => {};

const meta = {
  title: 'Login/SpacePickerStage',
  component: SpacePickerStage,
  tags: ['autodocs'],
} satisfies Meta<typeof SpacePickerStage>;

export default meta;

export const Default = {
  render: () => {
    setMock('list_my_spaces', spaces);
    setMock('set_active_space', null);
    return <SpacePickerStage onSuccess={noop} />;
  },
};

export const SingleSpace = {
  render: () => {
    setMock('list_my_spaces', [spaces[0]]);
    setMock('set_active_space', null);
    return <SpacePickerStage onSuccess={noop} />;
  },
};

export const NoSpaces = {
  render: () => {
    setMock('list_my_spaces', []);
    setMock('set_active_space', null);
    return <SpacePickerStage onSuccess={noop} />;
  },
};
