import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import { BulkActionBar } from './BulkActionBar';

const noop = () => {};

const categories = ['Groceries', 'Income', 'Transport', 'Entertainment'];

const setupMocks = () => {
  setMock('bulk_approve_transactions', null);
  setMock('bulk_categorize_transactions', null);
  setMock('classify_transactions', [
    'Groceries',
    'Income',
    '',
    'Entertainment',
  ]);
};

const meta = {
  title: 'Transactions/BulkActionBar',
  component: BulkActionBar,
} satisfies Meta<typeof BulkActionBar>;

export default meta;

export const SingleSelected = {
  render: () => {
    setupMocks();
    return (
      <BulkActionBar
        selectedIds={['1']}
        categories={categories}
        isDemoMode={false}
        onClear={noop}
      />
    );
  },
};

export const MultipleSelected = {
  render: () => {
    setupMocks();
    return (
      <BulkActionBar
        selectedIds={['1', '2', '3', '4']}
        categories={categories}
        isDemoMode={false}
        onClear={noop}
      />
    );
  },
};

export const DemoMode = {
  render: () => {
    setupMocks();
    return (
      <BulkActionBar
        selectedIds={['1', '2']}
        categories={categories}
        isDemoMode
        onClear={noop}
      />
    );
  },
};
