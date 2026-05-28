import type { Account } from '@/types';
import { AccountRow } from './AccountRow';

interface AccountsTableProps {
  accounts: Account[];
}

export const AccountsTable = ({
  accounts,
}: AccountsTableProps): React.JSX.Element => (
  <table className="w-full text-sm border-collapse">
    <thead>
      <tr className="border-b border-foreground/10 bg-foreground">
        <th className="px-6 py-3 text-left text-xs font-mono uppercase tracking-widest text-canvas/60">
          Name
        </th>
        <th className="px-6 py-3 text-left text-xs font-mono uppercase tracking-widest text-canvas/60">
          Type
        </th>
        <th className="px-6 py-3 text-left text-xs font-mono uppercase tracking-widest text-canvas/60">
          Source
        </th>
        <th className="px-6 py-3 text-right text-xs font-mono uppercase tracking-widest text-canvas/60">
          Currency
        </th>
        <th className="px-6 py-3 text-right text-xs font-mono uppercase tracking-widest text-canvas/60">
          Actions
        </th>
      </tr>
    </thead>
    <tbody>
      {accounts.map((account) => (
        <AccountRow key={account.id} account={account} />
      ))}
    </tbody>
  </table>
);
