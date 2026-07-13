import type { Meta } from 'storybook-react-rsbuild';
import type { Account } from '@/types';
import { AccountsTable } from './AccountsTable';

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
  {
    id: 'acct-2',
    name: 'Savings',
    currency: 'SEK',
    account_type: 'savings',
    account_source: 'import',
    color: '#3b82f6',
    space_id: 'space-1',
  },
  {
    id: 'acct-3',
    name: 'Investment',
    currency: 'USD',
    account_type: 'investment',
    account_source: 'sync',
    color: '#f97316',
    space_id: 'space-1',
  },
];

const meta = {
  title: 'Accounts/AccountsTable',
  component: AccountsTable,
  tags: ['autodocs'],
} satisfies Meta<typeof AccountsTable>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-xl">
      <AccountsTable accounts={accounts} />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="w-xl">
      <AccountsTable accounts={[]} />
    </div>
  ),
};

export const SingleAccount = {
  render: () => (
    <div className="w-xl">
      <AccountsTable accounts={[accounts[0]]} />
    </div>
  ),
};
