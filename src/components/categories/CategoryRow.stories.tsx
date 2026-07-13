import type { Meta } from 'storybook-react-rsbuild';
import type { Category } from '@/types';
import { CategoryRow } from './CategoryRow';

const categories: Category[] = [
  { id: '1', name: 'Groceries', color: '#22c55e' },
  { id: '2', name: 'Rent', color: '#3b82f6' },
  { id: '3', name: 'Other', color: '#6b7280' },
];

const noop = async () => {};

const meta = {
  title: 'Categories/CategoryRow',
  component: CategoryRow,
  tags: ['autodocs'],
} satisfies Meta<typeof CategoryRow>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-lg bg-canvas border border-border-subtle">
      <CategoryRow
        cat={categories[0]}
        isDeletePending={false}
        isUpdatePending={false}
        onDelete={noop}
        onUpdate={noop}
      />
    </div>
  ),
};

export const SystemDefault = {
  render: () => (
    <div className="w-lg bg-canvas border border-border-subtle">
      <CategoryRow
        cat={categories[2]}
        isDeletePending={false}
        isUpdatePending={false}
        onDelete={noop}
        onUpdate={noop}
      />
    </div>
  ),
};

export const DeletePending = {
  render: () => (
    <div className="w-lg bg-canvas border border-border-subtle">
      <CategoryRow
        cat={categories[1]}
        isDeletePending
        isUpdatePending={false}
        onDelete={noop}
        onUpdate={noop}
      />
    </div>
  ),
};

export const UpdatePending = {
  render: () => (
    <div className="w-lg bg-canvas border border-border-subtle">
      <CategoryRow
        cat={categories[0]}
        isDeletePending={false}
        isUpdatePending
        onDelete={noop}
        onUpdate={noop}
      />
    </div>
  ),
};

export const AllRows = {
  render: () => (
    <div className="w-lg bg-canvas border border-border-subtle divide-y divide-border-subtle">
      {categories.map((cat) => (
        <CategoryRow
          key={cat.id}
          cat={cat}
          isDeletePending={false}
          isUpdatePending={false}
          onDelete={noop}
          onUpdate={noop}
        />
      ))}
    </div>
  ),
};
