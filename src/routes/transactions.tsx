import { useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { listAllCategories } from '@/api/categories';
import { listTransactions } from '@/api/transactions';
import { BulkActionBar } from '@/components/transactions/BulkActionBar';
import {
  type FilterState,
  TransactionFilters,
} from '@/components/transactions/TransactionFilters';
import { TransactionTable } from '@/components/transactions/TransactionTable';
import { UploadDialog } from '@/components/transactions/UploadDialog';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { Pagination } from '@/components/ui/Pagination';
import { getDemoCategories, getDemoTransactions } from '@/demo/generators';
import { useDemoMode } from '@/hooks/useDemoMode';
import { useTransactionSelection } from '@/hooks/useTransactionSelection';
import { updateCategoryColors } from '@/lib/category-color';
import { cn } from '@/lib/util';

const PAGE_SIZE = 15;

function Transactions(): React.JSX.Element {
  const { isDemoMode } = useDemoMode();
  const [page, setPage] = useState(0);
  const [filters, setFilters] = useState<FilterState>({
    search: '',
    approved: 'all',
    dateFrom: '',
    dateTo: '',
    category: 'all',
  });
  const selection = useTransactionSelection();

  const [debouncedSearch, setDebouncedSearch] = useState('');
  useEffect(() => {
    const t = setTimeout(() => setDebouncedSearch(filters.search), 300);
    return () => clearTimeout(t);
  }, [filters.search]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll to top on page change
  useEffect(() => {
    window.scrollTo({ top: 0, behavior: 'instant' });
  }, [page]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset page to 0 on any filter search/change
  useEffect(() => {
    setPage(0);
  }, [
    debouncedSearch,
    filters.approved,
    filters.category,
    filters.dateFrom,
    filters.dateTo,
  ]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: clear selection on page or filter change
  useEffect(() => {
    selection.clear();
  }, [
    page,
    debouncedSearch,
    filters.approved,
    filters.category,
    filters.dateFrom,
    filters.dateTo,
  ]);

  const { data, isLoading, isError, isFetching } = useQuery({
    queryKey: [
      'transactions',
      isDemoMode ? 'demo' : 'live',
      page,
      debouncedSearch,
      filters.approved,
      filters.category,
      filters.dateFrom,
      filters.dateTo,
    ],
    queryFn: isDemoMode
      ? () =>
          getDemoTransactions({
            page,
            limit: PAGE_SIZE,
            search: debouncedSearch || undefined,
            approved:
              filters.approved === 'all'
                ? undefined
                : filters.approved === 'approved',
            category: filters.category === 'all' ? undefined : filters.category,
            dateFrom: filters.dateFrom || undefined,
            dateTo: filters.dateTo || undefined,
          })
      : () =>
          listTransactions({
            page,
            limit: PAGE_SIZE,
            search: debouncedSearch || undefined,
            approved:
              filters.approved === 'all'
                ? undefined
                : filters.approved === 'approved',
            category: filters.category === 'all' ? undefined : filters.category,
            dateFrom: filters.dateFrom || undefined,
            dateTo: filters.dateTo || undefined,
          }),
    enabled: true,
    placeholderData: (prev) => prev,
  });

  const { data: categoriesData = [] } = useQuery({
    queryKey: ['categories', isDemoMode ? 'demo' : 'real'],
    queryFn: isDemoMode ? getDemoCategories : listAllCategories,
  });
  if (categoriesData.length > 0) {
    updateCategoryColors(categoriesData);
  }
  const categories = categoriesData.map((cat) => cat.name);

  const transactions = data?.transactions ?? [];
  const pageCount = data?.page_count ?? 1;

  return (
    <div className="max-w-[1200px] mx-auto px-6 py-10 flex flex-col gap-6">
      <div className="flex items-center justify-between animate-fade-in">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            Transactions
          </h1>
          <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
            {isLoading
              ? 'Loading…'
              : `${data ? pageCount * PAGE_SIZE : 0}+ entries`}
          </p>
        </div>
        <UploadDialog />
      </div>

      <div className="animate-fade-in">
        <TransactionFilters
          filters={filters}
          onChange={setFilters}
          categories={categories}
        />
      </div>

      <div
        className={cn(
          'animate-fade-in transition-opacity duration-150',
          isFetching && 'opacity-60',
        )}
      >
        <Card>
          <CardHeader
            label="Transactions"
            meta={
              isLoading || transactions.length === 0
                ? undefined
                : `Page ${page + 1} of ${pageCount}`
            }
          />
          <CardBody className="p-0">
            {isError && (
              <p className="px-6 py-8 text-xs font-mono text-expense text-center">
                Failed to load transactions.
              </p>
            )}

            {!isError && (transactions.length > 0 || isLoading) && (
              <TransactionTable
                transactions={transactions}
                categories={categories}
                isLoading={isLoading}
                isDemoMode={isDemoMode}
                selectedIds={selection.selectedIds}
                onToggleRow={selection.toggle}
                onTogglePage={selection.toggleAll}
              />
            )}

            {!isLoading && !isError && transactions.length === 0 && (
              <p className="px-6 py-8 text-xs font-mono text-muted-foreground text-center">
                No transactions found.
              </p>
            )}
          </CardBody>
        </Card>
      </div>

      {/* Pagination */}
      <Pagination page={page} pageCount={pageCount} onPageChange={setPage} />

      <BulkActionBar
        selectedIds={selection.selectedIds}
        categories={categories}
        isDemoMode={isDemoMode}
        onClear={selection.clear}
      />
    </div>
  );
}

export const Route = createFileRoute('/transactions')({
  component: Transactions,
});
