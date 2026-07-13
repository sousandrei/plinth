import type { Meta } from 'storybook-react-rsbuild';
import type { Transaction } from '@/types';
import { TransactionTable } from './TransactionTable';

const noop = () => {};

const transactions: Transaction[] = [
  {
    id: '1',
    booking_date: '2025-03-01',
    value_date: '2025-03-02',
    reference: 'REF001',
    text: 'ICA Maxi Stockholm',
    currency: 'SEK',
    amount: -45230,
    balance: 12340,
    approved: true,
    note: '',
    category: 'Groceries',
    account_id: 'acct-1',
  },
  {
    id: '2',
    booking_date: '2025-03-03',
    value_date: '2025-03-03',
    reference: 'REF002',
    text: 'Salary March',
    currency: 'SEK',
    amount: 350000,
    balance: 362340,
    approved: true,
    note: 'Monthly salary',
    category: 'Income',
    account_id: 'acct-1',
  },
  {
    id: '3',
    booking_date: '2025-03-05',
    value_date: '2025-03-05',
    reference: 'REF003',
    text: 'SL Access Monthly Pass',
    currency: 'SEK',
    amount: -8900,
    balance: 353440,
    approved: false,
    note: '',
    category: null,
    account_id: 'acct-1',
  },
  {
    id: '4',
    booking_date: '2025-03-08',
    value_date: '2025-03-10',
    reference: 'REF004',
    text: 'Netflix Subscription',
    currency: 'SEK',
    amount: -13900,
    balance: 339540,
    approved: true,
    note: '',
    category: 'Entertainment',
    account_id: 'acct-1',
  },
];

const categories = ['Groceries', 'Income', 'Transport', 'Entertainment'];

const meta = {
  title: 'Transactions/TransactionTable',
  component: TransactionTable,
  tags: ['autodocs'],
} satisfies Meta<typeof TransactionTable>;

export default meta;

export const Default = {
  render: () => (
    <TransactionTable
      transactions={transactions}
      categories={categories}
      isLoading={false}
      isDemoMode={false}
      selectedIds={[]}
      onToggleRow={noop}
      onTogglePage={noop}
    />
  ),
};

export const Loading = {
  render: () => (
    <TransactionTable
      transactions={[]}
      categories={categories}
      isLoading
      isDemoMode={false}
      selectedIds={[]}
      onToggleRow={noop}
      onTogglePage={noop}
    />
  ),
};

export const Empty = {
  render: () => (
    <TransactionTable
      transactions={[]}
      categories={categories}
      isLoading={false}
      isDemoMode={false}
      selectedIds={[]}
      onToggleRow={noop}
      onTogglePage={noop}
    />
  ),
};

export const AllSelected = {
  render: () => (
    <TransactionTable
      transactions={transactions}
      categories={categories}
      isLoading={false}
      isDemoMode={false}
      selectedIds={['1', '2', '3', '4']}
      onToggleRow={noop}
      onTogglePage={noop}
    />
  ),
};

export const SomeSelected = {
  render: () => (
    <TransactionTable
      transactions={transactions}
      categories={categories}
      isLoading={false}
      isDemoMode={false}
      selectedIds={['1', '3']}
      onToggleRow={noop}
      onTogglePage={noop}
    />
  ),
};

export const SingleRow = {
  render: () => (
    <TransactionTable
      transactions={[transactions[0]]}
      categories={categories}
      isLoading={false}
      isDemoMode={false}
      selectedIds={[]}
      onToggleRow={noop}
      onTogglePage={noop}
    />
  ),
};
