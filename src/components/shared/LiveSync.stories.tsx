import {
  createMemoryHistory,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { useState } from 'react';
import { expect, fn, waitFor } from 'storybook/test';
import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { AuthProvider } from '@/context/AuthContext';
import { setMock } from '@/lib/tauri-mock';
import { routeTree } from '@/routeTree.gen';
import type {
  Account,
  TrainingSample,
  Transaction,
  TransactionPage,
} from '@/types';

const spaceId = 'household';
const transaction: Transaction = {
  id: 'purchase',
  booking_date: '2026-01-15',
  value_date: '2026-01-15',
  reference: '',
  text: 'Market',
  currency: 'SEK',
  amount: -1500,
  balance: 9000,
  approved: true,
  note: 'Original note',
  category: null,
  account_id: 'checking',
};

interface SyncWindowProps {
  path: '/' | '/transactions' | '/training';
}

const SyncWindow = ({ path }: SyncWindowProps): React.JSX.Element => {
  const [router] = useState(() =>
    createRouter({
      routeTree,
      history: createMemoryHistory({ initialEntries: [path] }),
    }),
  );
  return (
    <AuthProvider>
      <RouterProvider router={router} />
    </AuthProvider>
  );
};

const meta = {
  title: 'Sync/Live Views',
  component: SyncWindow,
  args: { path: '/transactions' },
  beforeEach: () => {
    setMock('get_session', {
      space_id: spaceId,
      user: {
        id: 'user',
        name: 'Alice',
        has_pin: true,
        created_at: '2026-01-01',
        updated_at: '2026-01-01',
      },
    });
    setMock('sync_space_revision', 0);
  },
} satisfies Meta<typeof SyncWindow>;

export default meta;
type Story = StoryObj<typeof meta>;

export const TransactionsRefresh: Story = {
  beforeEach: () => {
    setMock('list_transactions', {
      transactions: [transaction],
      page_count: 1,
    });
  },
  play: async ({ canvas }) => {
    await waitFor(() =>
      expect(canvas.getByText('Original note')).toBeVisible(),
    );
    const updated: TransactionPage = {
      transactions: [{ ...transaction, note: 'Edited on another device' }],
      page_count: 1,
    };
    let finishRefresh = () => {};
    const result = new Promise<TransactionPage>((resolve) => {
      finishRefresh = () => resolve(updated);
    });
    const refetch = fn(() => result);
    setMock('list_transactions', refetch);
    setMock('sync_space_revision', 1);
    try {
      await waitFor(() => expect(refetch).toHaveBeenCalled(), {
        timeout: 7000,
      });
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
      });
      const note = canvas.getByText('Original note');
      await expect(note).toBeVisible();
      await expect(note.closest('.opacity-60')).toBeNull();
    } finally {
      finishRefresh();
    }
    await waitFor(
      () => expect(canvas.getByText('Edited on another device')).toBeVisible(),
      { timeout: 7000 },
    );
    await expect(canvas.queryByText('Original note')).not.toBeInTheDocument();
  },
};

const checking: Account = {
  id: transaction.account_id,
  name: 'Checking',
  currency: 'SEK',
  account_type: 'checking',
  account_source: 'manual',
  color: '#306e65',
  space_id: spaceId,
};

const spending = (category: string) => ({
  '2026-01': {
    balance: { [checking.id]: transaction.balance },
    by_category: { [category]: transaction.amount },
  },
});

export const DashboardChartRefreshes: Story = {
  args: { path: '/' },
  beforeEach: () => {
    setMock('list_accounts', [checking]);
    setMock('get_aggregations', spending('Groceries'));
  },
  play: async ({ canvas }) => {
    await waitFor(() => expect(canvas.getByText('Groceries')).toBeVisible());
    setMock('get_aggregations', spending('Transport'));
    setMock('sync_space_revision', 1);
    await waitFor(() => expect(canvas.getByText('Transport')).toBeVisible(), {
      timeout: 7000,
    });
    await expect(canvas.queryByText('Groceries')).not.toBeInTheDocument();
  },
};

const sample: TrainingSample = {
  id: transaction.id,
  text: transaction.text,
  amount: transaction.amount,
  booking_date: transaction.booking_date,
  actual_category: 'Groceries',
  predicted_category: 'Groceries',
};

export const TrainingDataRefreshes: Story = {
  args: { path: '/training' },
  beforeEach: () => {
    setMock('is_classifier_ready', true);
    setMock('minilm_status', true);
    setMock('count_approved_transactions', 1);
    setMock('get_training_samples', [sample]);
  },
  play: async ({ canvas }) => {
    await waitFor(() => {
      expect(canvas.getByText('1 approved')).toBeVisible();
      expect(canvas.getByText('Market')).toBeVisible();
    });
    setMock('count_approved_transactions', 0);
    setMock('get_training_samples', []);
    setMock('sync_space_revision', 1);
    await waitFor(
      () => {
        expect(canvas.getByText('0 approved')).toBeVisible();
        expect(canvas.getByText('No approved transactions yet')).toBeVisible();
      },
      { timeout: 7000 },
    );
  },
};
