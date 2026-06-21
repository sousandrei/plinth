import { toast } from '@/components/ui/Toast';
import { cn } from '@/lib/util';

interface LogItem {
  id: string;
  text: string;
}

interface ConsoleLogsProps {
  logsList: LogItem[];
  logsHeight: number;
  draggingPanel: 'horizontal' | 'logs' | null;
}

export function ConsoleLogs({
  logsList,
  logsHeight,
  draggingPanel,
}: ConsoleLogsProps): React.JSX.Element {
  return (
    <div
      style={{ height: `${logsHeight}px` }}
      className="flex flex-col bg-[#121212] shrink-0 selection:bg-[#27272a] rounded-none"
    >
      <div className="h-8 flex items-center justify-between px-4 border-b border-[#27272a] bg-[#18181b] shrink-0 rounded-none">
        <span className="text-[10px] font-mono uppercase tracking-[0.1em] text-[#a1a1aa] font-bold">
          Console Log Outputs
        </span>
        {logsList.length > 0 && (
          <button
            type="button"
            onClick={() => {
              navigator.clipboard.writeText(
                logsList.map((l) => l.text).join('\n'),
              );
              toast.success('Copied to clipboard');
            }}
            className="text-[10px] font-mono text-[#a1a1aa] hover:text-white cursor-pointer transition-colors"
          >
            Copy Logs
          </button>
        )}
      </div>

      <div
        className={cn(
          'flex-1 p-4 overflow-auto font-mono text-[10px] leading-relaxed text-[#e4e4e7] whitespace-pre-wrap selection:bg-[#27272a] rounded-none',
          draggingPanel && 'pointer-events-none',
        )}
      >
        {logsList.length > 0 ? (
          logsList.map((log, idx) => (
            <div
              key={log.id}
              className="mb-0.5 border-b border-[#27272a] pb-0.5 last:border-b-0"
            >
              <span className="text-[#71717a] mr-2">[{idx + 1}]</span>
              {log.text}
            </div>
          ))
        ) : (
          <span className="text-[#71717a]">
            No custom console.log statements printed during execution.
          </span>
        )}
      </div>
    </div>
  );
}
