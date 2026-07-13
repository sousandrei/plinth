import type { Meta } from 'storybook-react-rsbuild';
import type { FinetuneProgress, ModelCard } from '@/types';
import { ModelDetail } from './ModelDetail';

const epochHistory: FinetuneProgress[] = [
  {
    epoch: 1,
    total_epochs: 12,
    train_loss: 0.85,
    train_accuracy: 0.72,
    val_loss: 0.88,
    val_accuracy: 0.68,
  },
  {
    epoch: 2,
    total_epochs: 12,
    train_loss: 0.62,
    train_accuracy: 0.79,
    val_loss: 0.7,
    val_accuracy: 0.74,
  },
  {
    epoch: 3,
    total_epochs: 12,
    train_loss: 0.48,
    train_accuracy: 0.84,
    val_loss: 0.55,
    val_accuracy: 0.79,
  },
  {
    epoch: 4,
    total_epochs: 12,
    train_loss: 0.38,
    train_accuracy: 0.88,
    val_loss: 0.45,
    val_accuracy: 0.83,
  },
  {
    epoch: 5,
    total_epochs: 12,
    train_loss: 0.3,
    train_accuracy: 0.9,
    val_loss: 0.38,
    val_accuracy: 0.85,
  },
];

const model: ModelCard = {
  version: 2,
  is_active: true,
  trained_at: '2025-02-18T09:15:00Z',
  samples_used: 2100,
  epochs: 12,
  train_loss: 0.21,
  train_accuracy: 0.93,
  val_loss: 0.28,
  val_accuracy: 0.91,
  epoch_history: epochHistory,
};

const meta = {
  title: 'Training/ModelDetail',
  component: ModelDetail,
  tags: ['autodocs'],
} satisfies Meta<typeof ModelDetail>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-lg">
      <ModelDetail model={model} />
    </div>
  ),
};

export const NoModel = {
  render: () => (
    <div className="w-lg">
      <ModelDetail model={null} />
    </div>
  ),
};
