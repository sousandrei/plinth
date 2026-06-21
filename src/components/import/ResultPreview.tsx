import JsonView from '@uiw/react-json-view';
import { vscodeTheme } from '@uiw/react-json-view/vscode';
import { Tabs } from '@/components/ui/Tabs';
import { toast } from '@/components/ui/Toast';
import { cn } from '@/lib/util';
import { AccountSummaryCard } from './AccountSummaryCard';
import { ParsedTransactionsTable } from './ParsedTransactionsTable';

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

interface TestOutput {
  account_id: string;
  month?: string;
  balance?: number;
  transactions?: ParsedTx[];
}

interface ResultPreviewProps {
  viewMode: 'table' | 'json';
  onViewModeChange: (mode: 'table' | 'json') => void;
  parsedResult: TestOutput | null;
  rawResult: string;
  errorMsg: string;
  testPending: boolean;
  draggingPanel: 'horizontal' | 'logs' | null;
}

export function ResultPreview({
  viewMode,
  onViewModeChange,
  parsedResult,
  rawResult,
  errorMsg,
  testPending,
  draggingPanel,
}: ResultPreviewProps): React.JSX.Element {
  return (
    <div className="flex-1 border-b border-border-muted flex flex-col bg-canvas overflow-hidden min-h-[140px] rounded-none">
      <Tabs.Root
        value={viewMode}
        onValueChange={(val) => onViewModeChange(val as 'table' | 'json')}
      >
        <Tabs.List>
          <Tabs.Tab value="table">Preview Table</Tabs.Tab>
          <Tabs.Tab value="json">Raw JSON</Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel
          value="table"
          className="flex-1 overflow-auto min-h-0 rounded-none relative"
        >
          {parsedResult ? (
            <div className="divide-y divide-border-subtle">
              {/* Always display the summary if balance or month is present */}
              {(parsedResult.balance !== undefined ||
                parsedResult.month !== undefined) && (
                <AccountSummaryCard
                  accountId={parsedResult.account_id}
                  month={parsedResult.month}
                  balance={parsedResult.balance}
                />
              )}

              {/* Display transactions if they exist */}
              {parsedResult.transactions &&
              parsedResult.transactions.length > 0 ? (
                <ParsedTransactionsTable
                  transactions={parsedResult.transactions}
                />
              ) : (
                <div className="p-4">
                  <p className="text-xs font-mono text-muted-foreground">
                    No transaction data included in this statement format.
                  </p>
                </div>
              )}
            </div>
          ) : (
            <div className="h-full flex items-center justify-center text-xs font-mono text-muted-foreground p-8 text-center rounded-none">
              {testPending
                ? 'Executing script transform...'
                : 'No transaction data. Select a statement and run transform to populate.'}
            </div>
          )}
        </Tabs.Panel>

        <Tabs.Panel
          value="json"
          className="flex-1 min-h-0 rounded-none bg-[#1e1e1e] font-mono text-[10px] leading-relaxed relative flex flex-col"
        >
          <div className="flex justify-end p-2 border-b border-[#2d2d2d] bg-[#252526] z-10 shrink-0">
            {rawResult && (
              <button
                type="button"
                onClick={() => {
                  navigator.clipboard.writeText(rawResult);
                  toast.success('Copied to clipboard');
                }}
                className="text-[10px] bg-[#1e1e1e] border border-[#3e3e42] hover:border-[#6b7280] text-[#cccccc] px-2 py-1 rounded-none cursor-pointer transition-colors font-mono font-bold"
              >
                Copy JSON
              </button>
            )}
          </div>
          <div
            className={cn(
              'flex-1 p-4 overflow-auto rounded-none bg-[#1e1e1e]',
              draggingPanel && 'pointer-events-none',
            )}
          >
            {errorMsg ? (
              <div className="text-[#f87171] whitespace-pre-wrap font-mono text-xs">
                <p className="font-bold text-xs uppercase mb-1">
                  Execution Failure
                </p>
                {errorMsg}
              </div>
            ) : parsedResult ? (
              <JsonView
                value={parsedResult}
                displayDataTypes={false}
                displayObjectSize={false}
                enableClipboard={false}
                style={{
                  ...vscodeTheme,
                  fontFamily: 'Geist Mono, Courier New, monospace',
                  fontSize: '11px',
                }}
              />
            ) : (
              <span className="text-muted-foreground font-mono">
                Waiting for raw execution output...
              </span>
            )}
          </div>
        </Tabs.Panel>
      </Tabs.Root>
    </div>
  );
}
