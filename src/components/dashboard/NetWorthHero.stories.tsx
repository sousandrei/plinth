import type { Meta } from 'storybook-react-rsbuild';
import { demoNetWorthSeries } from '@/demo/dashboard';
import { NetWorthHero } from './NetWorthHero';

const meta = {
  title: 'Dashboard/NetWorthHero',
  component: NetWorthHero,
  tags: ['autodocs'],
} satisfies Meta<typeof NetWorthHero>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md h-56">
      <NetWorthHero
        series={demoNetWorthSeries}
        latestNetWorth={demoNetWorthSeries[demoNetWorthSeries.length - 1].value}
        currency="SEK"
      />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="w-md h-56">
      <NetWorthHero series={[]} latestNetWorth={0} currency="SEK" />
    </div>
  ),
};

export const SingleMonth = {
  render: () => (
    <div className="w-md h-56">
      <NetWorthHero
        series={[{ month: '2026-05', value: 534000 }]}
        latestNetWorth={534000}
        currency="SEK"
      />
    </div>
  ),
};
