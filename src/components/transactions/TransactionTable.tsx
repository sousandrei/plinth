import { cn } from '@/lib/util';
import type { Transaction } from '@/types';
import { TransactionRow } from './TransactionRow';

interface TransactionTableProps {
  transactions: Transaction[];
  categories: string[];
  isLoading: boolean;
}

const COLS = [
  { label: '', width: 'w-[40px] min-w-[40px]' },
  { label: 'Date', width: 'w-[15%] min-w-[120px]' },
  { label: 'Description', width: 'w-[40%] min-w-[280px]' },
  { label: 'Category', width: 'w-[25%] min-w-[180px]' },
  { label: 'Amount', width: 'w-[16%] min-w-[130px]' },
] as const;

const ALIGNS = ['center', 'left', 'left', 'left', 'right'] as const;

export const TransactionTable = ({
  transactions,
  categories,
  isLoading,
}: TransactionTableProps): React.JSX.Element => (
  <table className="w-full text-sm border-collapse table-fixed">
    <thead className="sticky top-0 z-10">
      <tr className="border-b border-foreground/10 bg-foreground">
        {COLS.map((col, i) => (
          <th
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed column order
            key={i}
            className={cn(
              'px-6 py-3 text-xs font-mono uppercase tracking-widest text-canvas/60',
              col.width,
              ALIGNS[i] === 'right' ? 'text-right' : 'text-left',
            )}
          >
            {col.label}
          </th>
        ))}
      </tr>
    </thead>
    <tbody>
      {isLoading &&
        Array.from({ length: 8 }).map((_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: skeleton placeholder
          <tr key={i} className="border-b border-border-subtle">
            <td className="px-3 py-3 w-[40px] min-w-[40px] text-center">
              <div className="w-3 h-3 rounded-full bg-muted animate-pulse mx-auto" />
            </td>
            <td className="px-6 py-3 w-[15%] min-w-[120px]">
              <div className="h-3 w-20 bg-muted animate-pulse" />
            </td>
            <td className="px-6 py-3 w-[43%] min-w-[280px]">
              <div className="h-3 w-56 bg-muted animate-pulse" />
            </td>
            <td className="px-6 py-3 w-[26%] min-w-[180px]">
              <div className="h-3 w-16 bg-muted animate-pulse" />
            </td>
            <td className="px-6 py-3 text-right w-[16%] min-w-[130px]">
              <div className="h-3 w-20 bg-muted animate-pulse ml-auto" />
            </td>
          </tr>
        ))}
      {!isLoading &&
        transactions.map((t) => (
          <TransactionRow key={t.id} transaction={t} categories={categories} />
        ))}
    </tbody>
  </table>
);
