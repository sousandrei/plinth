import type { Meta } from 'storybook-react-rsbuild';
import { Button } from './Button';
import { Tooltip } from './Tooltip';

const meta = {
  title: 'UI/Tooltip',
  component: Tooltip,
  tags: ['autodocs'],
} satisfies Meta<typeof Tooltip>;

export default meta;

export const Default = {
  render: () => (
    <Tooltip content="This action cannot be undone.">
      <Button variant="ghost">Hover me</Button>
    </Tooltip>
  ),
};

export const LongContent = {
  render: () => (
    <Tooltip content="Deleting this account will remove all associated transactions and cannot be reversed. Please confirm before proceeding.">
      <Button variant="ghost">Delete account</Button>
    </Tooltip>
  ),
};

export const CustomDelay = {
  render: () => (
    <Tooltip content="Appears after 100ms." delay={100}>
      <Button variant="ghost">Quick tooltip</Button>
    </Tooltip>
  ),
};
