import type { Meta } from 'storybook-react-rsbuild';
import { setMock } from '@/lib/tauri-mock';
import type { Account, AccountSummary } from '@/types';
import { AccountSummaryRow } from './AccountSummaryRow';

const account: Account = {
  id: 'acct-1',
  name: 'Checking',
  currency: 'SEK',
  account_type: 'checking',
  account_source: 'manual',
  color: '#22c55e',
  space_id: 'space-1',
};

const summary: AccountSummary = {
  month: '2025-03',
  account_id: 'acct-1',
  balance: 452300,
};

const setupMocks = () => {
  setMock('upsert_account_summary', null);
  setMock('delete_account_summary', null);
};

const meta = {
  title: 'AccountSummaries/AccountSummaryRow',
  component: AccountSummaryRow,
  tags: ['autodocs'],
} satisfies Meta<typeof AccountSummaryRow>;

export default meta;

export const Default = {
  render: () => {
    setupMocks();
    return (
      <table className="w-full">
        <tbody>
          <AccountSummaryRow summary={summary} account={account} />
        </tbody>
      </table>
    );
  },
};

export const NoAccount = {
  render: () => {
    setupMocks();
    return (
      <table className="w-full">
        <tbody>
          <AccountSummaryRow summary={summary} account={undefined} />
        </tbody>
      </table>
    );
  },
};
