import type { Meta } from 'storybook-react-rsbuild';
import { demoAccountSeries } from '@/demo/dashboard';
import { AccountsTable } from './AccountsTable';

const meta = {
  title: 'Dashboard/AccountsTable',
  component: AccountsTable,
  tags: ['autodocs'],
} satisfies Meta<typeof AccountsTable>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-lg">
      <AccountsTable accountSeries={demoAccountSeries} currency="SEK" />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="w-lg">
      <AccountsTable accountSeries={[]} currency="SEK" />
    </div>
  ),
};

export const SingleAccount = {
  render: () => (
    <div className="w-lg">
      <AccountsTable accountSeries={[demoAccountSeries[0]]} currency="SEK" />
    </div>
  ),
};
