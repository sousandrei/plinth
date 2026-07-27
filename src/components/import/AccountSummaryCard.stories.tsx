import type { Meta } from 'storybook-react-rsbuild';
import { AccountSummaryCard } from './AccountSummaryCard';

const meta = {
  title: 'Import/AccountSummaryCard',
  component: AccountSummaryCard,
  tags: ['autodocs'],
} satisfies Meta<typeof AccountSummaryCard>;

export default meta;

export const Default = {
  render: () => (
    <AccountSummaryCard
      accountId="acct-123"
      accountType="investment"
      month="2025-03"
      balance={452300}
    />
  ),
};

export const NoMonth = {
  render: () => (
    <AccountSummaryCard
      accountId="acct-123"
      accountType="savings"
      balance={452300}
    />
  ),
};

export const NoBalance = {
  render: () => (
    <AccountSummaryCard
      accountId="acct-123"
      accountType="checking"
      month="2025-03"
    />
  ),
};

export const AccountIdOnly = {
  render: () => <AccountSummaryCard accountId="acct-123" />,
};
