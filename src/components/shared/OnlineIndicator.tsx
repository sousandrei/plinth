import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { listPeers } from '@/api/sync';
import { cn } from '@/lib/util';

interface OnlineIndicatorProps {
  className?: string;
}

const ACTIVE_THRESHOLD_SECS = 4;
const TICK_INTERVAL_MS = 1000;

type PeerStatus = 'active' | 'inactive';

const peerStatus = (lastSeen: number, now: number): PeerStatus =>
  now - lastSeen <= ACTIVE_THRESHOLD_SECS ? 'active' : 'inactive';

export const OnlineIndicator = ({
  className,
}: OnlineIndicatorProps): React.JSX.Element | null => {
  const { data: peers = [] } = useQuery({
    queryKey: ['peers'],
    queryFn: listPeers,
    refetchInterval: 1000,
  });

  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    const id = setInterval(
      () => setNow(Math.floor(Date.now() / 1000)),
      TICK_INTERVAL_MS,
    );
    return () => clearInterval(id);
  }, []);

  if (peers.length === 0) return null;

  const activeCount = peers.filter(
    (p) => peerStatus(p.last_seen, now) === 'active',
  ).length;
  const totalCount = peers.length;
  const hasActive = activeCount > 0;

  const dotClass = hasActive ? 'bg-emerald-500' : 'bg-amber-500';
  const headerLabel = hasActive
    ? `Online Devices — ${activeCount}`
    : `Inactive Devices — ${totalCount}`;

  return (
    <div className="relative group flex items-center self-stretch">
      <button
        type="button"
        aria-label={hasActive ? 'Show online devices' : 'Show inactive devices'}
        className={cn(
          'flex items-center justify-center p-2 cursor-pointer bg-transparent border-none',
          className,
        )}
      >
        <span
          role="img"
          aria-label={
            hasActive
              ? 'Another Plinth device is online on your local network'
              : 'No Plinth device has been seen recently on your local network'
          }
          title={
            hasActive
              ? 'Another Plinth device is online on your local network'
              : 'No Plinth device seen recently — last activity over 4s ago'
          }
          className={cn(
            'inline-block w-2 h-2 rounded-full ring-2 ring-canvas-raised shadow-sm',
            dotClass,
          )}
        />
      </button>

      <div className="absolute top-full right-0 w-64 bg-canvas-raised border border-border-muted shadow-lg z-50 hidden group-hover:flex group-focus-within:flex flex-col py-1 animate-in fade-in slide-in-from-top-1 duration-150">
        <div className="px-4 py-2.5 border-b border-border-subtle">
          <p className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-semibold">
            {headerLabel}
          </p>
        </div>
        <ul className="max-h-64 overflow-y-auto">
          {peers.map((peer) => {
            const status = peerStatus(peer.last_seen, now);
            const secondsAgo = now - peer.last_seen;
            return (
              <li
                key={peer.device_id}
                className="px-4 py-2 text-xs font-mono text-muted-foreground hover:bg-muted/60 transition-colors duration-150"
              >
                <div className="flex items-center gap-2">
                  <span
                    className={cn(
                      'w-1.5 h-1.5 rounded-full shrink-0',
                      status === 'active' ? 'bg-emerald-500' : 'bg-amber-500',
                    )}
                  />
                  <span className="text-foreground truncate flex-1">
                    {peer.name}
                  </span>
                </div>
                <p className="text-[10px] text-muted-foreground mt-0.5 truncate pl-3.5">
                  {peer.host}:{peer.port}
                  {' · '}
                  {status === 'active'
                    ? `${secondsAgo}s ago`
                    : `inactive ${secondsAgo}s`}
                </p>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
};
