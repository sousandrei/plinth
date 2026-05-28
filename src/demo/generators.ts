import type {
  AccountSummary,
  Category,
  ListTransactionsParams,
  ModelCard,
  TransactionPage,
} from '@/types';
import { DEMO_ACCOUNTS, DEMO_CATEGORIES, getGeneratedData } from './data';

export { DEMO_ACCOUNTS, DEMO_CATEGORIES };

export const getDemoAccounts = () => Promise.resolve([...DEMO_ACCOUNTS]);

export const getDemoCategories = (): Promise<Category[]> =>
  Promise.resolve([...DEMO_CATEGORIES]);

export const getDemoTransactions = (
  params: ListTransactionsParams,
): Promise<TransactionPage> => {
  const { transactions } = getGeneratedData();
  const page = params.page ?? 0;
  const limit = params.limit ?? 50;
  const search = params.search?.toLowerCase() ?? '';
  const { approved, category, dateFrom, dateTo } = params;

  let rows = [...transactions];

  if (search) {
    rows = rows.filter(
      (t) =>
        t.text.toLowerCase().includes(search) ||
        (t.category?.toLowerCase().includes(search) ?? false),
    );
  }
  if (approved !== undefined) {
    rows = rows.filter((t) => t.approved === approved);
  }
  if (category) {
    rows = rows.filter((t) => t.category === category);
  }
  if (dateFrom) {
    rows = rows.filter((t) => t.booking_date >= dateFrom);
  }
  if (dateTo) {
    rows = rows.filter((t) => t.booking_date <= dateTo);
  }

  const page_count = Math.max(1, Math.ceil(rows.length / limit));
  const start = page * limit;
  return Promise.resolve({
    transactions: rows.slice(start, start + limit),
    page_count,
  });
};

export const getDemoAggregations = () =>
  Promise.resolve({ ...getGeneratedData().aggregations });

export const getDemoAccountSummaries = (
  _userId: string,
): Promise<AccountSummary[]> => {
  const { aggregations } = getGeneratedData();
  const summaries: AccountSummary[] = Object.entries(aggregations).flatMap(
    ([month, agg]) =>
      Object.entries(agg.balance).map(([account_id, balance]) => ({
        month,
        account_id,
        balance,
      })),
  );
  return Promise.resolve(summaries);
};

export const getDemoModels = (): Promise<ModelCard[]> =>
  Promise.resolve([...getGeneratedData().models]);
