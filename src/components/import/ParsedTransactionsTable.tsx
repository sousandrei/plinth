import { categoryChipStyle } from '@/lib/category-color';
import { cn } from '@/lib/util';

interface ParsedTx {
  id: string;
  booking_date: string;
  value_date: string;
  reference: string;
  text: string;
  amount: number;
  balance: number;
  category?: string;
}

interface ParsedTransactionsTableProps {
  transactions: ParsedTx[];
}

export function ParsedTransactionsTable({
  transactions,
}: ParsedTransactionsTableProps): React.JSX.Element {
  return (
    <div className="flex flex-col">
      <div className="p-4 pb-2 bg-canvas-raised border-b border-border-subtle shrink-0">
        <p className="text-[10px] font-mono uppercase tracking-wider text-muted-foreground font-bold">
          Statement Transactions ({transactions.length})
        </p>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-left border-collapse rounded-none">
          <thead className="sticky top-0 bg-canvas-raised z-10 border-b border-border-subtle">
            <tr className="text-[10px] font-mono text-muted-foreground uppercase rounded-none">
              <th className="py-2 px-4 font-semibold bg-canvas-raised">
                Booking Date
              </th>
              <th className="py-2 px-4 font-semibold bg-canvas-raised">
                Reference
              </th>
              <th className="py-2 px-4 font-semibold bg-canvas-raised">Text</th>
              <th className="py-2 px-4 font-semibold bg-canvas-raised">
                Category
              </th>
              <th className="py-2 px-4 font-semibold text-right bg-canvas-raised">
                Amount
              </th>
              <th className="py-2 px-4 font-semibold text-right bg-canvas-raised">
                Balance
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-subtle text-xs rounded-none">
            {transactions.map((tx, idx) => (
              <tr key={tx.id || idx} className="hover:bg-muted/30">
                <td className="py-2 px-4 font-mono">{tx.booking_date}</td>
                <td className="py-2 px-4 font-mono text-muted-foreground max-w-[120px] truncate">
                  {tx.reference}
                </td>
                <td className="py-2 px-4 truncate max-w-[180px]">{tx.text}</td>
                <td className="py-2 px-4 font-mono">
                  {tx.category ? (
                    <span
                      className="inline-flex items-center px-2 py-0.5 text-[10px] uppercase tracking-widest font-mono border whitespace-nowrap"
                      style={categoryChipStyle(tx.category)}
                    >
                      {tx.category}
                    </span>
                  ) : (
                    <span className="text-muted-foreground italic text-[10px]">
                      —
                    </span>
                  )}
                </td>
                <td
                  className={cn(
                    'py-2 px-4 font-mono text-right',
                    tx.amount < 0
                      ? 'text-expense font-bold'
                      : 'text-growth font-bold',
                  )}
                >
                  {(tx.amount / 100).toFixed(2)}
                </td>
                <td className="py-2 px-4 font-mono text-right text-muted-foreground">
                  {(tx.balance / 100).toFixed(2)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
