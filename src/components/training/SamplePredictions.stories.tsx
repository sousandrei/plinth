import type { Meta } from 'storybook-react-rsbuild';
import type { TrainingSample } from '@/types';
import { SamplePredictions } from './SamplePredictions';

const noop = () => {};

const samples: TrainingSample[] = [
  {
    id: '1',
    text: 'ICA Maxi Stockholm',
    amount: -45230,
    booking_date: '2025-03-01',
    actual_category: 'Groceries',
    predicted_category: 'Groceries',
  },
  {
    id: '2',
    text: 'SL Access Monthly',
    amount: -8900,
    booking_date: '2025-03-02',
    actual_category: 'Transport',
    predicted_category: 'Entertainment',
  },
  {
    id: '3',
    text: 'Netflix Subscription',
    amount: -13900,
    booking_date: '2025-03-05',
    actual_category: 'Entertainment',
    predicted_category: 'Entertainment',
  },
  {
    id: '4',
    text: 'Hemkop',
    amount: -23450,
    booking_date: '2025-03-08',
    actual_category: 'Groceries',
    predicted_category: 'Rent',
  },
];

const meta = {
  title: 'Training/SamplePredictions',
  component: SamplePredictions,
  tags: ['autodocs'],
} satisfies Meta<typeof SamplePredictions>;

export default meta;

export const Default = {
  render: () => (
    <div className="w-md">
      <SamplePredictions
        samples={samples}
        isLoading={false}
        isClassifierReady
        onRefresh={noop}
      />
    </div>
  ),
};

export const Loading = {
  render: () => (
    <div className="w-md">
      <SamplePredictions
        samples={[]}
        isLoading
        isClassifierReady
        onRefresh={noop}
      />
    </div>
  ),
};

export const ClassifierNotReady = {
  render: () => (
    <div className="w-md">
      <SamplePredictions
        samples={[]}
        isLoading={false}
        isClassifierReady={false}
        onRefresh={noop}
      />
    </div>
  ),
};

export const NoApprovedTransactions = {
  render: () => (
    <div className="w-md">
      <SamplePredictions
        samples={[]}
        isLoading={false}
        isClassifierReady
        onRefresh={noop}
      />
    </div>
  ),
};

export const AllCorrect = {
  render: () => (
    <div className="w-md">
      <SamplePredictions
        samples={samples.filter(
          (s) => s.actual_category === s.predicted_category,
        )}
        isLoading={false}
        isClassifierReady
        onRefresh={noop}
      />
    </div>
  ),
};
