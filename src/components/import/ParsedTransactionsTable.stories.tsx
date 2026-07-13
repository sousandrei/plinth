import type { Meta } from 'storybook-react-rsbuild';
import { ParsedTransactionsTable } from './ParsedTransactionsTable';

const transactions = [
  {
    id: '1',
    booking_date: '2025-03-01',
    value_date: '2025-03-02',
    reference: 'REF001',
    text: 'ICA Maxi',
    amount: -45230,
    balance: 12340,
    category: 'Groceries',
  },
  {
    id: '2',
    booking_date: '2025-03-03',
    value_date: '2025-03-03',
    reference: 'REF002',
    text: 'Salary',
    amount: 350000,
    balance: 362340,
  },
  {
    id: '3',
    booking_date: '2025-03-05',
    value_date: '2025-03-05',
    reference: 'REF003',
    text: 'SL Access',
    amount: -8900,
    balance: 353440,
    category: 'Transport',
  },
];

const meta = {
  title: 'Import/ParsedTransactionsTable',
  component: ParsedTransactionsTable,
  tags: ['autodocs'],
} satisfies Meta<typeof ParsedTransactionsTable>;

export default meta;

export const Default = {
  render: () => <ParsedTransactionsTable transactions={transactions} />,
};

export const Empty = {
  render: () => <ParsedTransactionsTable transactions={[]} />,
};

export const SingleRow = {
  render: () => <ParsedTransactionsTable transactions={[transactions[0]]} />,
};
