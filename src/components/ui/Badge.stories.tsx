import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { Badge } from './Badge';

const meta = {
  title: 'UI/Badge',
  component: Badge,
  tags: ['autodocs'],
  args: { children: 'Label' },
} satisfies Meta<typeof Badge>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Muted: Story = { args: { variant: 'muted' } };

export const Accent: Story = { args: { variant: 'accent' } };

export const Growth: Story = { args: { variant: 'growth' } };

export const Expense: Story = { args: { variant: 'expense' } };

export const Highlight: Story = { args: { variant: 'highlight' } };

export const AllVariants: Story = {
  render: () => (
    <div className="flex flex-wrap gap-3">
      <Badge variant="default">default</Badge>
      <Badge variant="muted">muted</Badge>
      <Badge variant="accent">accent</Badge>
      <Badge variant="growth">growth</Badge>
      <Badge variant="expense">expense</Badge>
      <Badge variant="highlight">highlight</Badge>
    </div>
  ),
};
