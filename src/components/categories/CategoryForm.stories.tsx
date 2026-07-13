import type { Meta } from 'storybook-react-rsbuild';
import { CategoryForm } from './CategoryForm';

const noop = () => {};

const meta = {
  title: 'Categories/CategoryForm',
  component: CategoryForm,
  tags: ['autodocs'],
} satisfies Meta<typeof CategoryForm>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md">
      <CategoryForm isPending={false} onSubmit={noop} />
    </div>
  ),
};

export const Pending = {
  render: () => (
    <div className="w-md">
      <CategoryForm isPending onSubmit={noop} />
    </div>
  ),
};
