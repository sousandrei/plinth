import type { Meta } from 'storybook-react-rsbuild';
import type { ModelCard } from '@/types';
import { ModelCardList } from './ModelCardList';

const baseModel: ModelCard = {
  version: 0,
  is_base: true,
  is_active: true,
  trained_at: 'shipped',
  samples_used: 0,
  epochs: 0,
  train_loss: null,
  train_accuracy: 0.81,
  val_loss: null,
  val_accuracy: 0.81,
  epoch_history: [],
};

const trainedModels: ModelCard[] = [
  {
    version: 1,
    is_base: false,
    is_active: false,
    trained_at: '2025-01-10T14:30:00Z',
    samples_used: 1240,
    epochs: 8,
    train_loss: 0.32,
    train_accuracy: 0.89,
    val_loss: 0.41,
    val_accuracy: 0.85,
    epoch_history: [],
  },
  {
    version: 2,
    is_base: false,
    is_active: true,
    trained_at: '2025-02-18T09:15:00Z',
    samples_used: 2100,
    epochs: 12,
    train_loss: 0.21,
    train_accuracy: 0.93,
    val_loss: 0.28,
    val_accuracy: 0.91,
    epoch_history: [],
  },
  {
    version: 3,
    is_base: false,
    is_active: false,
    trained_at: '2025-03-22T16:45:00Z',
    samples_used: 3450,
    epochs: 15,
    train_loss: 0.15,
    train_accuracy: 0.96,
    val_loss: 0.22,
    val_accuracy: 0.93,
    epoch_history: [],
  },
];

const allModels = [baseModel, ...trainedModels];

const noop = () => {};

const meta = {
  title: 'Training/ModelCardList',
  component: ModelCardList,
  tags: ['autodocs'],
} satisfies Meta<typeof ModelCardList>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md">
      <ModelCardList
        models={allModels}
        isLoading={false}
        selectedVersion={2}
        onSelect={noop}
        onActivate={noop}
        onDelete={noop}
      />
    </div>
  ),
};

export const Loading = {
  render: () => (
    <div className="w-md">
      <ModelCardList
        models={[]}
        isLoading
        selectedVersion={null}
        onSelect={noop}
        onActivate={noop}
        onDelete={noop}
      />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="w-md">
      <ModelCardList
        models={[]}
        isLoading={false}
        selectedVersion={null}
        onSelect={noop}
        onActivate={noop}
        onDelete={noop}
      />
    </div>
  ),
};

export const BaseOnly = {
  render: () => (
    <div className="w-md">
      <ModelCardList
        models={[baseModel]}
        isLoading={false}
        selectedVersion={0}
        onSelect={noop}
        onActivate={noop}
        onDelete={noop}
      />
    </div>
  ),
};

export const NoneSelected = {
  render: () => (
    <div className="w-md">
      <ModelCardList
        models={allModels}
        isLoading={false}
        selectedVersion={null}
        onSelect={noop}
        onActivate={noop}
        onDelete={noop}
      />
    </div>
  ),
};
