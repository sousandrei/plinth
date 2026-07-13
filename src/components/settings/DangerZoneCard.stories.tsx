import type { Meta } from 'storybook-react-rsbuild';
import { DangerZoneCard } from './DangerZoneCard';

const noop = () => {};

const meta = {
  title: 'Settings/DangerZoneCard',
  component: DangerZoneCard,
  tags: ['autodocs'],
} satisfies Meta<typeof DangerZoneCard>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md">
      <DangerZoneCard onReset={noop} isPending={false} />
    </div>
  ),
};

export const Resetting = {
  render: () => (
    <div className="w-md">
      <DangerZoneCard onReset={noop} isPending />
    </div>
  ),
};
