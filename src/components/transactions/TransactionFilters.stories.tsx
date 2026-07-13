import type { Meta } from 'storybook-react-rsbuild';
import type { FilterState } from './TransactionFilters';
import { TransactionFilters } from './TransactionFilters';

const noop = () => {};

const defaultFilters: FilterState = {
  search: '',
  approved: 'all',
  dateFrom: '',
  dateTo: '',
  category: 'all',
};

const categories = ['Groceries', 'Rent', 'Transport', 'Entertainment'];

const meta = {
  title: 'Transactions/TransactionFilters',
  component: TransactionFilters,
  tags: ['autodocs'],
} satisfies Meta<typeof TransactionFilters>;

export default meta;

export const Default = {
  render: () => (
    <TransactionFilters
      filters={defaultFilters}
      onChange={noop}
      categories={categories}
    />
  ),
};

export const WithSearchAndCategory = {
  render: () => (
    <TransactionFilters
      filters={{
        ...defaultFilters,
        search: 'ica',
        category: 'Groceries',
      }}
      onChange={noop}
      categories={categories}
    />
  ),
};

export const ApprovedOnly = {
  render: () => (
    <TransactionFilters
      filters={{ ...defaultFilters, approved: 'approved' }}
      onChange={noop}
      categories={categories}
    />
  ),
};

export const DateRange = {
  render: () => (
    <TransactionFilters
      filters={{
        ...defaultFilters,
        dateFrom: '2025-01-01',
        dateTo: '2025-03-31',
      }}
      onChange={noop}
      categories={categories}
    />
  ),
};

export const AllFiltersActive = {
  render: () => (
    <TransactionFilters
      filters={{
        search: 'salary',
        approved: 'unapproved',
        dateFrom: '2025-01-01',
        dateTo: '2025-06-30',
        category: 'Rent',
      }}
      onChange={noop}
      categories={categories}
    />
  ),
};
