import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { acceptPairTokenFromPeer, getDeviceName, listPeers } from '@/api/sync';
import { Button } from '@/components/ui/Button';
import { cn } from '@/lib/util';
import type { PeerInfo } from '@/types';

type JoinStep = 'peer' | 'token';

interface JoinSpaceModalProps {
  onClose: () => void;
  onJoined: (spaceId: string) => void;
}

export const JoinSpaceModal = ({
  onClose,
  onJoined,
}: JoinSpaceModalProps): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [step, setStep] = useState<JoinStep>('peer');
  const [selectedPeer, setSelectedPeer] = useState<PeerInfo | null>(null);
  const [token, setToken] = useState('');
  const [tokenError, setTokenError] = useState('');

  const { data: deviceName = 'This device' } = useQuery({
    queryKey: ['device-name'],
    queryFn: getDeviceName,
    staleTime: Infinity,
  });

  const {
    data: peers = [],
    isLoading: peersLoading,
    refetch,
  } = useQuery({
    queryKey: ['peers'],
    queryFn: listPeers,
    refetchInterval: 3000,
  });

  const joinMutation = useMutation({
    mutationFn: () => {
      if (!selectedPeer) throw new Error('No peer selected');
      return acceptPairTokenFromPeer(
        selectedPeer.device_id,
        token.trim(),
        deviceName,
      );
    },
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      onJoined(result.space_id);
    },
    onError: (e) => setTokenError(String(e)),
  });

  const submitToken = (e: React.FormEvent) => {
    e.preventDefault();
    if (!token.trim()) {
      setTokenError('Token is required');
      return;
    }
    setTokenError('');
    joinMutation.mutate();
  };

  return (
    <div className="flex flex-col gap-5">
      {step === 'peer' && (
        <>
          <p className="text-xs text-muted-foreground">
            Select a device on your local network to join one of its spaces.
          </p>
          {peersLoading ? (
            <span className="text-xs font-mono text-muted-foreground">
              Scanning…
            </span>
          ) : peers.length === 0 ? (
            <div className="flex flex-col gap-2">
              <span className="text-xs font-mono text-muted-foreground">
                No devices found on this network.
              </span>
              <button
                type="button"
                onClick={() => refetch()}
                className="self-start text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
              >
                Retry
              </button>
            </div>
          ) : (
            <div className="flex flex-col divide-y divide-border-subtle border border-border-muted">
              {peers.map((peer) => (
                <button
                  key={peer.device_id}
                  type="button"
                  onClick={() => {
                    setSelectedPeer(peer);
                    setStep('token');
                  }}
                  className={cn(
                    'flex items-center gap-3 px-4 py-3 text-left text-sm font-mono',
                    'transition-all duration-150',
                    'hover:bg-accent-muted/20',
                  )}
                >
                  <span className="w-2 h-2 rounded-full bg-accent shrink-0" />
                  <span className="flex-1 truncate">
                    {peer.name || peer.host}
                  </span>
                  <span className="text-[10px] text-muted-foreground shrink-0">
                    {peer.host}
                  </span>
                </button>
              ))}
            </div>
          )}
          <div className="flex justify-end pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Cancel
            </Button>
          </div>
        </>
      )}

      {step === 'token' && (
        <>
          <p className="text-xs text-muted-foreground">
            On{' '}
            <span className="font-mono text-foreground">
              {selectedPeer?.host}
            </span>
            , open{' '}
            <span className="font-mono">
              Spaces → Edit → Devices → Pair Device
            </span>{' '}
            and generate a token.
          </p>
          <form onSubmit={submitToken} className="flex flex-col gap-3">
            <input
              // biome-ignore lint/a11y/noAutofocus: intentional — only input in step
              autoFocus
              value={token}
              onChange={(e) =>
                setToken(e.target.value.replace(/\D/g, '').slice(0, 6))
              }
              placeholder="000000"
              inputMode="numeric"
              maxLength={6}
              className={cn(
                'w-full px-4 py-2.5 text-center text-2xl font-mono tracking-[0.4em]',
                'bg-canvas border border-border-muted',
                'placeholder:text-muted-foreground placeholder:tracking-normal placeholder:text-sm',
                'focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
                'transition-all duration-150',
                tokenError && 'border-expense',
              )}
            />
            {tokenError && (
              <p className="text-xs font-mono text-expense">{tokenError}</p>
            )}
            <div className="flex justify-between pt-2 border-t border-border-subtle">
              <Button
                variant="ghost"
                type="button"
                onClick={() => {
                  setToken('');
                  setTokenError('');
                  setStep('peer');
                }}
                className="px-3 text-xs rounded-none h-9"
              >
                ← Back
              </Button>
              <Button
                type="submit"
                disabled={token.length < 6 || joinMutation.isPending}
                className="px-4 text-xs rounded-none h-9"
              >
                {joinMutation.isPending ? 'Joining…' : 'Join Space'}
              </Button>
            </div>
          </form>
        </>
      )}
    </div>
  );
};
