import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import { setOpenMock } from '@/lib/tauri-plugin-mocks';
import type { ImportResult, Space } from '@/types';
import { ImportDialog } from './ImportDialog';

const noop = () => {};

const spaces: Space[] = [
  {
    id: 'space-1',
    name: 'Personal',
    role: 'owner',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'space-2',
    name: 'Shared',
    role: 'member',
    created_at: '2025-02-01T00:00:00Z',
    updated_at: '2025-02-01T00:00:00Z',
  },
];

const importResult: ImportResult = {
  categories: 8,
  accounts: 3,
  transactions: 142,
  account_summaries: 36,
};

const setupMocks = () => {
  setMock('create_space', spaces[0]);
  setMock('import_space_data', importResult);
  setOpenMock(() => Promise.resolve('/path/to/export.json'));
};

const meta = {
  title: 'Spaces/ImportDialog',
  component: ImportDialog,
  tags: ['autodocs'],
} satisfies Meta<typeof ImportDialog>;

export default meta;

export const Empty = {
  render: () => {
    setupMocks();
    return (
      <div className="w-md">
        <ImportDialog spaces={spaces} onClose={noop} />
      </div>
    );
  },
};

export const FileSelectedNewSpace = {
  render: () => {
    setupMocks();
    return (
      <div className="w-md">
        <ImportDialog
          spaces={spaces}
          onClose={noop}
          initialFilePath="/path/to/export.json"
        />
      </div>
    );
  },
};

export const FileSelectedExistingSpace = {
  render: () => {
    setupMocks();
    return (
      <div className="w-md">
        <ImportDialog
          spaces={spaces}
          onClose={noop}
          initialFilePath="/path/to/export.json"
          initialMode="existing"
        />
      </div>
    );
  },
};

export const ImportComplete = {
  render: () => {
    setupMocks();
    return (
      <div className="w-md">
        <ImportDialog
          spaces={spaces}
          onClose={noop}
          initialResult={importResult}
        />
      </div>
    );
  },
};
