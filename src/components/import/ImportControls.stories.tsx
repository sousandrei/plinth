import type { Meta } from 'storybook-react-rsbuild';
import type { ParserFileInfo } from '@/api/import';
import { ImportControls } from './ImportControls';

const noop = () => {};

const files: ParserFileInfo[] = [
  {
    filename: 'nordea.py',
    path: '/parsers/nordea.py',
    is_builtin: true,
    content: '',
    units: [
      {
        key: 'checking',
        name: 'Checking Account',
        bank: 'Nordea',
        format: 'csv',
        account_type: 'checking',
        account_source: 'nordea',
        is_builtin: true,
      },
      {
        key: 'savings',
        name: 'Savings Account',
        bank: 'Nordea',
        format: 'csv',
        account_type: 'savings',
        account_source: 'nordea',
        is_builtin: true,
      },
    ],
  },
  {
    filename: 'swedbank.py',
    path: '/parsers/swedbank.py',
    is_builtin: false,
    content: '',
    units: [
      {
        key: 'default',
        name: 'Default',
        bank: 'Swedbank',
        format: 'xls',
        account_type: 'checking',
        account_source: 'swedbank',
        is_builtin: false,
      },
    ],
  },
];

const meta = {
  title: 'Import/ImportControls',
  component: ImportControls,
  tags: ['autodocs'],
} satisfies Meta<typeof ImportControls>;

export default meta;

export const Default = {
  render: () => (
    <ImportControls
      files={files}
      loadingFiles={false}
      selectedFile={files[0]}
      onSelectedFileChange={noop}
      targetUnit="checking"
      onTargetUnitChange={noop}
      targetFile="/path/to/statement.csv"
      onBrowseFile={noop}
      onRunTest={noop}
      onSaveScript={noop}
      testPending={false}
      savePending={false}
    />
  ),
};

export const NoFileSelected = {
  render: () => (
    <ImportControls
      files={files}
      loadingFiles={false}
      selectedFile={null}
      onSelectedFileChange={noop}
      targetUnit=""
      onTargetUnitChange={noop}
      targetFile=""
      onBrowseFile={noop}
      onRunTest={noop}
      onSaveScript={noop}
      testPending={false}
      savePending={false}
    />
  ),
};

export const LoadingFiles = {
  render: () => (
    <ImportControls
      files={undefined}
      loadingFiles
      selectedFile={null}
      onSelectedFileChange={noop}
      targetUnit=""
      onTargetUnitChange={noop}
      targetFile=""
      onBrowseFile={noop}
      onRunTest={noop}
      onSaveScript={noop}
      testPending={false}
      savePending={false}
    />
  ),
};

export const TestPending = {
  render: () => (
    <ImportControls
      files={files}
      loadingFiles={false}
      selectedFile={files[0]}
      onSelectedFileChange={noop}
      targetUnit="checking"
      onTargetUnitChange={noop}
      targetFile="/path/to/statement.csv"
      onBrowseFile={noop}
      onRunTest={noop}
      onSaveScript={noop}
      testPending
      savePending={false}
    />
  ),
};

export const SavePending = {
  render: () => (
    <ImportControls
      files={files}
      loadingFiles={false}
      selectedFile={files[0]}
      onSelectedFileChange={noop}
      targetUnit="checking"
      onTargetUnitChange={noop}
      targetFile="/path/to/statement.csv"
      onBrowseFile={noop}
      onRunTest={noop}
      onSaveScript={noop}
      testPending={false}
      savePending
    />
  ),
};
