import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { Separator } from './Separator';

const meta = {
  title: 'UI/Separator',
  component: Separator,
  tags: ['autodocs'],
} satisfies Meta<typeof Separator>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Horizontal: Story = {
  render: () => (
    <div className="w-80">
      <p className="text-sm text-muted-foreground mb-4">Above the line</p>
      <Separator />
      <p className="text-sm text-muted-foreground mt-4">Below the line</p>
    </div>
  ),
};

export const Vertical: Story = {
  render: () => (
    <div className="flex items-center gap-6 h-16">
      <span className="text-sm text-muted-foreground">Left</span>
      <Separator orientation="vertical" className="h-12" />
      <span className="text-sm text-muted-foreground">Right</span>
    </div>
  ),
};
