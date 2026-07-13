import type { Meta } from 'storybook-react-rsbuild';
import type { UploadResult } from '@/api/upload';
import { setMock } from '@/lib/tauri-mock';
import { setOpenMock } from '@/lib/tauri-plugin-mocks';
import type { Account } from '@/types';
import { UploadDialog } from './UploadDialog';

const accounts: Account[] = [
  {
    id: 'acct-1',
    name: 'Checking',
    currency: 'SEK',
    account_type: 'checking',
    account_source: 'manual',
    color: '#22c55e',
    space_id: 'space-1',
  },
];

const uploadResults: UploadResult[] = [
  {
    account_id: 'acct-1',
    inserted: 42,
    skipped: 3,
    logs: [
      'Parsing statement...',
      'Found 45 rows',
      'Inserted 42, skipped 3 duplicates',
    ],
  },
];

const parsers = [
  {
    key: 'nordea-checking',
    name: 'Checking Account',
    bank: 'Nordea',
    format: 'csv',
    account_type: 'checking',
    account_source: 'nordea',
    is_builtin: true,
  },
  {
    key: 'swedbank-default',
    name: 'Default',
    bank: 'Swedbank',
    format: 'xlsx',
    account_type: 'checking',
    account_source: 'swedbank',
    is_builtin: false,
  },
];

const setupMocks = () => {
  setMock('list_parsers', parsers);
  setMock('list_accounts', accounts);
  setMock('upload_file', uploadResults[0]);
  setOpenMock(() => Promise.resolve(['/path/to/statement.csv']));
};

const meta = {
  title: 'Transactions/UploadDialog',
  component: UploadDialog,
} satisfies Meta<typeof UploadDialog>;

export default meta;

export const Default = {
  render: () => {
    setupMocks();
    return <UploadDialog />;
  },
};

export const FileSelected = {
  render: () => {
    setupMocks();
    return (
      <UploadDialog
        initialFilePaths={['/path/to/statement.csv']}
        initialParserKey="nordea-checking"
      />
    );
  },
};

export const UploadComplete = {
  render: () => {
    setupMocks();
    return <UploadDialog initialResults={uploadResults} />;
  },
};

export const UploadError = {
  render: () => {
    setupMocks();
    return (
      <UploadDialog initialErrorMsg="Failed to parse statement: invalid format" />
    );
  },
};
