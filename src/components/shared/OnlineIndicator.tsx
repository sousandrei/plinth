import { useQuery } from '@tanstack/react-query';
import { listPeers } from '@/api/sync';
import { cn } from '@/lib/util';

interface OnlineIndicatorProps {
  className?: string;
}

export const OnlineIndicator = ({
  className,
}: OnlineIndicatorProps): React.JSX.Element | null => {
  const { data: peers = [] } = useQuery({
    queryKey: ['peers'],
    queryFn: listPeers,
    refetchInterval: 3000,
  });

  if (peers.length === 0) return null;

  return (
    <div className="relative group flex items-center self-stretch">
      <button
        type="button"
        aria-label="Show online devices"
        className={cn(
          'flex items-center justify-center p-2 cursor-pointer bg-transparent border-none',
          className,
        )}
      >
        <span
          role="img"
          aria-label="Another Plinth device is online on your local network"
          title="Another Plinth device is online on your local network"
          className="inline-block w-2 h-2 rounded-full bg-emerald-500 ring-2 ring-canvas-raised shadow-sm"
        />
      </button>

      <div className="absolute top-full right-0 w-64 bg-canvas-raised border border-border-muted shadow-lg z-50 hidden group-hover:flex group-focus-within:flex flex-col py-1 animate-in fade-in slide-in-from-top-1 duration-150">
        <div className="px-4 py-2.5 border-b border-border-subtle">
          <p className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-semibold">
            Online Devices — {peers.length}
          </p>
        </div>
        <ul className="max-h-64 overflow-y-auto">
          {peers.map((peer) => (
            <li
              key={peer.device_id}
              className="px-4 py-2 text-xs font-mono text-muted-foreground hover:bg-muted/60 transition-colors duration-150"
            >
              <div className="flex items-center gap-2">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 shrink-0" />
                <span className="text-foreground truncate flex-1">
                  {peer.name}
                </span>
              </div>
              <p className="text-[10px] text-muted-foreground mt-0.5 truncate pl-3.5">
                {peer.host}:{peer.port}
              </p>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
};
