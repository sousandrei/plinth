import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { generatePairToken, getDeviceName, getLocalAddress } from '@/api/sync';
import { Button } from '@/components/ui/Button';
import { cn } from '@/lib/util';
import type { PairToken } from '@/types';

interface PairModalProps {
  onClose: () => void;
  initialToken?: PairToken | null;
}

export const PairModal = ({
  onClose,
  initialToken = null,
}: PairModalProps): React.JSX.Element => {
  const [token, setToken] = useState<PairToken | null>(initialToken);
  const [secondsLeft, setSecondsLeft] = useState(() =>
    initialToken
      ? Math.max(
          0,
          initialToken.expires_at_unix - Math.floor(Date.now() / 1000),
        )
      : 0,
  );

  const { data: deviceName = 'This device' } = useQuery({
    queryKey: ['device-name'],
    queryFn: getDeviceName,
    staleTime: Infinity,
  });

  const { data: localAddress } = useQuery({
    queryKey: ['local-address'],
    queryFn: getLocalAddress,
    staleTime: Infinity,
  });

  const generateMutation = useMutation({
    mutationFn: () => generatePairToken(deviceName),
    onSuccess: (pt) => {
      setToken(pt);
      const remaining = Math.max(
        0,
        pt.expires_at_unix - Math.floor(Date.now() / 1000),
      );
      setSecondsLeft(remaining);
      const iv = setInterval(() => {
        setSecondsLeft((s) => {
          if (s <= 1) {
            clearInterval(iv);
            return 0;
          }
          return s - 1;
        });
      }, 1000);
    },
  });

  return (
    <div className="flex flex-col gap-4">
      {token === null ? (
        <>
          <p className="text-xs text-muted-foreground">
            Generate a token on this device. The other device will find this one
            automatically on the local network and use the token to
            authenticate.
          </p>
          {generateMutation.isError && (
            <p className="text-xs font-mono text-expense">
              {String(generateMutation.error)}
            </p>
          )}
          <div className="flex justify-end gap-2 pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Cancel
            </Button>
            <Button
              onClick={() => generateMutation.mutate()}
              disabled={generateMutation.isPending}
              className="px-4 text-xs rounded-none h-9"
            >
              {generateMutation.isPending ? 'Generating…' : 'Generate Token'}
            </Button>
          </div>
        </>
      ) : (
        <>
          <div className="flex flex-col items-center gap-3 py-4">
            <span className="text-4xl font-mono font-bold tracking-[0.25em]">
              {token.token}
            </span>
            <span
              className={cn(
                'text-xs font-mono',
                secondsLeft > 10 ? 'text-muted-foreground' : 'text-expense',
              )}
            >
              {secondsLeft > 0 ? `Expires in ${secondsLeft}s` : 'Expired'}
            </span>
            <span className="text-[10px] font-mono text-muted-foreground">
              {deviceName}
              {localAddress ? ` · ${localAddress}` : ''}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            On the other device, choose{' '}
            <span className="font-mono text-foreground">Join via pairing</span>,
            select this device from the network list, then enter this token.
          </p>
          <div className="flex justify-end pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Done
            </Button>
          </div>
        </>
      )}
    </div>
  );
};
