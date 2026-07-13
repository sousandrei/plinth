import type { Meta } from 'storybook-react-rsbuild';
import type { EmbedProgress, FinetuneProgress } from '@/types';
import { ProgressChart } from './ProgressChart';

const history: FinetuneProgress[] = [
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
  {
    epoch: 6,
    total_epochs: 12,
    train_loss: 0.25,
    train_accuracy: 0.92,
    val_loss: 0.33,
    val_accuracy: 0.87,
  },
  {
    epoch: 7,
    total_epochs: 12,
    train_loss: 0.22,
    train_accuracy: 0.93,
    val_loss: 0.3,
    val_accuracy: 0.88,
  },
  {
    epoch: 8,
    total_epochs: 12,
    train_loss: 0.21,
    train_accuracy: 0.93,
    val_loss: 0.28,
    val_accuracy: 0.91,
  },
];

const embedProgress: EmbedProgress = { current: 450, total: 1200 };

const meta = {
  title: 'Training/ProgressChart',
  component: ProgressChart,
  tags: ['autodocs'],
} satisfies Meta<typeof ProgressChart>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-lg">
      <ProgressChart
        history={history}
        totalEpochs={12}
        isTraining={false}
        embedProgress={null}
      />
    </div>
  ),
};

export const Training = {
  render: () => (
    <div className="w-lg">
      <ProgressChart
        history={history}
        totalEpochs={12}
        isTraining
        embedProgress={null}
      />
    </div>
  ),
};

export const Embedding = {
  render: () => (
    <div className="w-lg">
      <ProgressChart
        history={[]}
        totalEpochs={12}
        isTraining
        embedProgress={embedProgress}
      />
    </div>
  ),
};

export const NoHistory = {
  render: () => (
    <div className="w-lg">
      <ProgressChart
        history={[]}
        totalEpochs={12}
        isTraining={false}
        embedProgress={null}
      />
    </div>
  ),
};

export const Starting = {
  render: () => (
    <div className="w-lg">
      <ProgressChart
        history={[]}
        totalEpochs={12}
        isTraining
        embedProgress={null}
      />
    </div>
  ),
};

export const SingleEpoch = {
  render: () => (
    <div className="w-lg">
      <ProgressChart
        history={[history[0]]}
        totalEpochs={12}
        isTraining
        embedProgress={null}
      />
    </div>
  ),
};
