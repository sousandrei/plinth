import type { Meta } from 'storybook-react-rsbuild';
import type { FinetuneResult } from '@/types';
import { TrainingControls } from './TrainingControls';

const noop = () => {};

const result: FinetuneResult = {
  version: 2,
  epochs_completed: 12,
  final_train_loss: 0.21,
  final_train_accuracy: 0.93,
  final_val_loss: 0.28,
  final_val_accuracy: 0.91,
  samples_used: 2100,
  epoch_history: [],
};

const meta = {
  title: 'Training/TrainingControls',
  component: TrainingControls,
  tags: ['autodocs'],
} satisfies Meta<typeof TrainingControls>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md">
      <TrainingControls
        approvedCount={150}
        isTraining={false}
        result={null}
        error={null}
        onStart={noop}
        onStop={noop}
      />
    </div>
  ),
};

export const Training = {
  render: () => (
    <div className="w-md">
      <TrainingControls
        approvedCount={150}
        isTraining
        result={null}
        error={null}
        onStart={noop}
        onStop={noop}
      />
    </div>
  ),
};

export const Stopping = {
  render: () => (
    <div className="w-md">
      <TrainingControls
        approvedCount={150}
        isTraining
        result={null}
        error={null}
        onStart={noop}
        onStop={noop}
      />
    </div>
  ),
};

export const WithResult = {
  render: () => (
    <div className="w-md">
      <TrainingControls
        approvedCount={150}
        isTraining={false}
        result={result}
        error={null}
        onStart={noop}
        onStop={noop}
      />
    </div>
  ),
};

export const WithError = {
  render: () => (
    <div className="w-md">
      <TrainingControls
        approvedCount={150}
        isTraining={false}
        result={null}
        error="CUDA out of memory. Try reducing batch size."
        onStart={noop}
        onStop={noop}
      />
    </div>
  ),
};

export const NotEnoughSamples = {
  render: () => (
    <div className="w-md">
      <TrainingControls
        approvedCount={5}
        isTraining={false}
        result={null}
        error={null}
        onStart={noop}
        onStop={noop}
      />
    </div>
  ),
};
