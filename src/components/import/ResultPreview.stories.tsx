import type { Meta } from 'storybook-react-rsbuild';
import { ResultPreview } from './ResultPreview';

const noop = () => {};

const parsedResult = {
  account_id: 'acct-123',
  month: '2025-03',
  balance: 452300,
  transactions: [
    {
      id: '1',
      booking_date: '2025-03-01',
      value_date: '2025-03-02',
      reference: 'REF001',
      text: 'ICA Maxi',
      amount: -45230,
      balance: 12340,
      category: 'Groceries',
    },
    {
      id: '2',
      booking_date: '2025-03-03',
      value_date: '2025-03-03',
      reference: 'REF002',
      text: 'Salary',
      amount: 350000,
      balance: 362340,
    },
  ],
};

const rawResult = JSON.stringify(parsedResult, null, 2);

const meta = {
  title: 'Import/ResultPreview',
  component: ResultPreview,
  tags: ['autodocs'],
} satisfies Meta<typeof ResultPreview>;

export default meta;

export const TableView = {
  render: () => (
    <div className="h-96 flex flex-col">
      <ResultPreview
        viewMode="table"
        onViewModeChange={noop}
        parsedResult={parsedResult}
        rawResult={rawResult}
        errorMsg=""
        testPending={false}
        draggingPanel={null}
      />
    </div>
  ),
};

export const JsonView = {
  render: () => (
    <div className="h-96 flex flex-col">
      <ResultPreview
        viewMode="json"
        onViewModeChange={noop}
        parsedResult={parsedResult}
        rawResult={rawResult}
        errorMsg=""
        testPending={false}
        draggingPanel={null}
      />
    </div>
  ),
};

export const Empty = {
  render: () => (
    <div className="h-96 flex flex-col">
      <ResultPreview
        viewMode="table"
        onViewModeChange={noop}
        parsedResult={null}
        rawResult=""
        errorMsg=""
        testPending={false}
        draggingPanel={null}
      />
    </div>
  ),
};

export const TestPending = {
  render: () => (
    <div className="h-96 flex flex-col">
      <ResultPreview
        viewMode="table"
        onViewModeChange={noop}
        parsedResult={null}
        rawResult=""
        errorMsg=""
        testPending
        draggingPanel={null}
      />
    </div>
  ),
};

export const WithError = {
  render: () => (
    <div className="h-96 flex flex-col">
      <ResultPreview
        viewMode="json"
        onViewModeChange={noop}
        parsedResult={null}
        rawResult=""
        errorMsg={
          'Traceback (most recent call last):\n  File "nordea.py", line 42, in parse\n    raise ValueError("Invalid date format")\nValueError: Invalid date format'
        }
        testPending={false}
        draggingPanel={null}
      />
    </div>
  ),
};
