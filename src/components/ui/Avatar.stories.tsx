import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { Avatar } from './Avatar';

const meta = {
  title: 'UI/Avatar',
  component: Avatar,
  tags: ['autodocs'],
  args: { name: 'Jane Doe' },
} satisfies Meta<typeof Avatar>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const SingleName: Story = {
  args: { name: 'Ada' },
};

export const LongName: Story = {
  args: { name: 'Alexandra Voronova-Stockholm' },
};
