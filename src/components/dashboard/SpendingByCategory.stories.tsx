import type { Meta } from 'storybook-react-rsbuild';
import { demoSpendingCategories, demoSpendingSeries } from '@/demo/dashboard';
import { SpendingByCategory } from './SpendingByCategory';

const meta = {
  title: 'Dashboard/SpendingByCategory',
  component: SpendingByCategory,
  tags: ['autodocs'],
} satisfies Meta<typeof SpendingByCategory>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-full h-72">
      <SpendingByCategory
        series={demoSpendingSeries}
        categories={demoSpendingCategories}
        currency="SEK"
      />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="w-full h-72">
      <SpendingByCategory series={[]} categories={[]} currency="SEK" />
    </div>
  ),
};
