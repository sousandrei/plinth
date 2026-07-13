import type { Meta } from 'storybook-react-rsbuild';
import { demoAccounts, demoAllocationSeries } from '@/demo/dashboard';
import { NetWorthAllocation } from './NetWorthAllocation';

const meta = {
  title: 'Dashboard/NetWorthAllocation',
  component: NetWorthAllocation,
  tags: ['autodocs'],
} satisfies Meta<typeof NetWorthAllocation>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-xs h-56">
      <NetWorthAllocation
        series={demoAllocationSeries}
        accounts={demoAccounts}
        currency="SEK"
      />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="w-xs h-56">
      <NetWorthAllocation series={[]} accounts={[]} currency="SEK" />
    </div>
  ),
};
