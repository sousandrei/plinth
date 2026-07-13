import { Checkbox } from '@/components/ui/Checkbox';
import { cn } from '@/lib/util';
import type { Transaction } from '@/types';
import { TransactionRow } from './TransactionRow';

interface TransactionTableProps {
  transactions: Transaction[];
  categories: string[];
  isLoading: boolean;
  isDemoMode: boolean;
  selectedIds: string[];
  onToggleRow: (id: string) => void;
  onTogglePage: (ids: string[], selectAll: boolean) => void;
}

const COLS = [
  { label: '', width: 'w-[40px] min-w-[40px]' },
  { label: 'Approved', width: 'w-[90px] min-w-[90px]' },
  { label: 'Date', width: 'w-[15%] min-w-[120px]' },
  { label: 'Description', width: 'w-[40%] min-w-[240px]' },
  { label: 'Category', width: 'w-[24%] min-w-[160px]' },
  { label: 'Amount', width: 'w-[16%] min-w-[130px]' },
] as const;

const ALIGNS = ['center', 'center', 'left', 'left', 'left', 'right'] as const;

export const TransactionTable = ({
  transactions,
  categories,
  isLoading,
  isDemoMode,
  selectedIds,
  onToggleRow,
  onTogglePage,
}: TransactionTableProps): React.JSX.Element => {
  const pageIds = transactions.map((t) => t.id);
  const allSelected =
    pageIds.length > 0 && pageIds.every((id) => selectedIds.includes(id));
  const someSelected =
    !allSelected && pageIds.some((id) => selectedIds.includes(id));

  return (
    <table className="w-full text-sm border-collapse table-fixed">
      <thead className="sticky top-0 z-10">
        <tr className="border-b border-foreground/10 bg-foreground">
          {COLS.map((col, i) => {
            const isSelectCol = i === 0;
            return (
              <th
                // biome-ignore lint/suspicious/noArrayIndexKey: fixed column order
                key={i}
                className={cn(
                  'px-6 py-3 text-xs font-mono uppercase tracking-widest text-canvas/60',
                  col.width,
                  ALIGNS[i] === 'right' ? 'text-right' : 'text-left',
                )}
              >
                {isSelectCol && !isLoading && pageIds.length > 0 ? (
                  <Checkbox
                    checked={allSelected}
                    indeterminate={someSelected}
                    onCheckedChange={() => onTogglePage(pageIds, !allSelected)}
                    label="Select all on page"
                  />
                ) : (
                  col.label
                )}
              </th>
            );
          })}
        </tr>
      </thead>
      <tbody>
        {isLoading &&
          Array.from({ length: 8 }).map((_, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: skeleton placeholder
            <tr key={i} className="border-b border-border-subtle">
              <td className="px-6 py-4 w-[40px] min-w-[40px] text-center">
                <div className="h-4 w-4 bg-muted animate-pulse mx-auto" />
              </td>
              <td className="px-6 py-4 w-[90px] min-w-[90px] text-center">
                <div className="h-5 w-9 bg-muted animate-pulse mx-auto rounded" />
              </td>
              <td className="px-6 py-4 w-[15%] min-w-[120px]">
                <div className="h-3 w-20 bg-muted animate-pulse" />
              </td>
              <td className="px-6 py-4 w-[40%] min-w-[240px]">
                <div className="h-3 w-56 bg-muted animate-pulse" />
              </td>
              <td className="px-6 py-4 w-[24%] min-w-[160px]">
                <div className="h-3 w-16 bg-muted animate-pulse" />
              </td>
              <td className="px-6 py-4 text-right w-[16%] min-w-[130px]">
                <div className="h-3 w-20 bg-muted animate-pulse ml-auto" />
              </td>
            </tr>
          ))}
        {!isLoading &&
          transactions.map((t) => (
            <TransactionRow
              key={t.id}
              transaction={t}
              categories={categories}
              selected={selectedIds.includes(t.id)}
              isDemoMode={isDemoMode}
              onToggleSelect={onToggleRow}
            />
          ))}
      </tbody>
    </table>
  );
};
