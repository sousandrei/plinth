import { formatAmount } from '@/components/transactions/TransactionRow';

interface AccountSummaryCardProps {
  accountId: string;
  month?: string;
  balance?: number;
}

export function AccountSummaryCard({
  accountId,
  month,
  balance,
}: AccountSummaryCardProps): React.JSX.Element {
  return (
    <div className="p-4 bg-canvas-raised border-b border-border-subtle">
      <div className="max-w-md p-4 border border-border-muted bg-canvas text-xs font-mono space-y-1.5 rounded-none">
        <p className="text-[10px] uppercase tracking-wider text-muted-foreground font-bold">
          Account Summary
        </p>
        <p>
          Account ID: <span className="text-foreground">{accountId}</span>
        </p>
        {month && (
          <p>
            Month: <span className="text-foreground">{month}</span>
          </p>
        )}
        {balance !== undefined && (
          <p>
            Balance:{' '}
            <span className="text-foreground font-semibold">
              {formatAmount(balance, 'SEK')}
            </span>
          </p>
        )}
      </div>
    </div>
  );
}
