import type { Meta } from 'storybook-react-rsbuild';
import { demoCashFlowSeries } from '@/demo/dashboard';
import { CashFlowChart } from './CashFlowChart';

const meta = {
  title: 'Dashboard/CashFlowChart',
  component: CashFlowChart,
  tags: ['autodocs'],
} satisfies Meta<typeof CashFlowChart>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md h-56">
      <CashFlowChart series={demoCashFlowSeries} currency="SEK" />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="w-md h-56">
      <CashFlowChart series={[]} currency="SEK" />
    </div>
  ),
};
