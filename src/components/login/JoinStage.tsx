import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { setActiveSpace } from '@/api/spaces';
import { acceptPairTokenFromPeer, listPeers } from '@/api/sync';
import { createUser, setPin } from '@/api/users';
import { cn } from '@/lib/util';
import type { JoinResult, PeerInfo, User } from '@/types';
import { PinInput } from './PinInput';

type JoinStep = 'peer' | 'name' | 'pin' | 'confirm' | 'token';

interface JoinStageProps {
  onSuccess: (user: User, spaceId: string) => void;
  onBack: () => void;
}

export const JoinStage = ({
  onSuccess,
  onBack,
}: JoinStageProps): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [step, setStep] = useState<JoinStep>('peer');
  const [selectedPeer, setSelectedPeer] = useState<PeerInfo | null>(null);
  const [name, setName] = useState('');
  const [nameError, setNameError] = useState('');
  const [pin, setPin_] = useState('');
  const [pinError, setPinError] = useState('');
  const [token, setToken] = useState('');
  const [tokenError, setTokenError] = useState('');
  const nameRef = useRef<HTMLInputElement>(null);
  const tokenRef = useRef<HTMLInputElement>(null);

  const {
    data: peers = [],
    isLoading: peersLoading,
    refetch,
  } = useQuery({
    queryKey: ['peers'],
    queryFn: listPeers,
    refetchInterval: 3000,
  });

  useEffect(() => {
    if (step === 'name') nameRef.current?.focus();
    if (step === 'token') tokenRef.current?.focus();
  }, [step]);

  const joinMutation = useMutation({
    mutationFn: async (): Promise<{ user: User; result: JoinResult }> => {
      if (!selectedPeer) throw new Error('No peer selected');
      const user = await createUser(name.trim());
      await setPin(user.id, pin);
      const result = await acceptPairTokenFromPeer(
        selectedPeer.device_id,
        token.trim(),
        name.trim(),
      );
      return { user, result };
    },
    onSuccess: async ({ user, result }) => {
      await setActiveSpace(result.space_id);
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      onSuccess(user, result.space_id);
    },
    onError: (e) => setTokenError(String(e)),
  });

  const submitName = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setNameError('Name is required');
      return;
    }
    setNameError('');
    setStep('pin');
  };

  const handlePinComplete = (p: string) => {
    setPin_(p);
    setStep('confirm');
  };

  const handleConfirmComplete = (p: string) => {
    if (p !== pin) {
      setPinError("PINs don't match — try again");
      setStep('pin');
      setPin_('');
      return;
    }
    setPinError('');
    setStep('token');
  };

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
    <div className="flex flex-col items-center gap-6 animate-slide-in-right">
      {/* Peer selection */}
      {step === 'peer' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">
              Join a Space
            </h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              Select a device on your local network.
            </p>
          </div>

          {peersLoading ? (
            <span className="text-xs font-mono text-muted-foreground">
              Scanning…
            </span>
          ) : peers.length === 0 ? (
            <div className="flex flex-col items-center gap-3">
              <span className="text-xs font-mono text-muted-foreground">
                No devices found on this network.
              </span>
              <button
                type="button"
                onClick={() => refetch()}
                className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
              >
                Retry
              </button>
            </div>
          ) : (
            <div className="flex flex-col gap-2 w-64">
              {peers.map((peer) => (
                <button
                  key={peer.device_id}
                  type="button"
                  onClick={() => {
                    setSelectedPeer(peer);
                    setStep('name');
                  }}
                  className={cn(
                    'flex items-center gap-3 px-4 py-3',
                    'bg-canvas-raised border border-border-subtle',
                    'text-left text-sm font-mono',
                    'transition-all duration-150',
                    'hover:border-accent hover:bg-accent-muted/20 hover:-translate-y-px',
                    'active:scale-[0.99]',
                  )}
                >
                  <span className="w-2 h-2 rounded-full bg-accent shrink-0" />
                  <span className="truncate">{peer.host}</span>
                  <span className="ml-auto text-[10px] text-muted-foreground">
                    :{peer.port}
                  </span>
                </button>
              ))}
            </div>
          )}

          <button
            type="button"
            onClick={onBack}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}

      {/* Name */}
      {step === 'name' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">Your name</h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              This is how you'll appear on the paired device.
            </p>
          </div>
          <form onSubmit={submitName} className="flex flex-col gap-3 w-64">
            <input
              ref={nameRef}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Your name"
              className={cn(
                'w-full px-4 py-2.5 text-sm font-mono',
                'bg-canvas border border-border-muted',
                'placeholder:text-muted-foreground',
                'focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
                'transition-all duration-150',
                nameError && 'border-expense',
              )}
            />
            {nameError && (
              <p className="text-xs font-mono text-expense">{nameError}</p>
            )}
            <button
              type="submit"
              className={cn(
                'w-full px-4 py-2.5 text-xs font-mono uppercase tracking-widest',
                'bg-foreground text-canvas',
                'transition-all duration-150 active:scale-[0.98] hover:opacity-90',
              )}
            >
              Continue
            </button>
          </form>
          <button
            type="button"
            onClick={() => setStep('peer')}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}

      {/* PIN */}
      {step === 'pin' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">
              Choose a PIN
            </h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              4-digit PIN to secure your data.
            </p>
          </div>
          <PinInput
            onComplete={handlePinComplete}
            error={pinError || undefined}
            key="join-pin"
          />
          <button
            type="button"
            onClick={() => {
              setPinError('');
              setStep('name');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}

      {/* Confirm PIN */}
      {step === 'confirm' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">
              Confirm PIN
            </h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              Enter your PIN again.
            </p>
          </div>
          <PinInput onComplete={handleConfirmComplete} key="join-confirm" />
          <button
            type="button"
            onClick={() => {
              setPin_('');
              setPinError('');
              setStep('pin');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}

      {/* Token entry */}
      {step === 'token' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">
              Enter token
            </h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              Enter the 6-digit token shown on{' '}
              <span className="text-foreground">{selectedPeer?.host}</span>.
            </p>
          </div>
          <form onSubmit={submitToken} className="flex flex-col gap-3 w-64">
            <input
              ref={tokenRef}
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
            <button
              type="submit"
              disabled={token.length < 6 || joinMutation.isPending}
              className={cn(
                'w-full px-4 py-2.5 text-xs font-mono uppercase tracking-widest',
                'bg-foreground text-canvas',
                'transition-all duration-150 active:scale-[0.98] hover:opacity-90',
                'disabled:opacity-50 disabled:pointer-events-none',
              )}
            >
              {joinMutation.isPending ? 'Joining…' : 'Join Space'}
            </button>
          </form>
          <button
            type="button"
            onClick={() => {
              setTokenError('');
              setToken('');
              setStep('confirm');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}
    </div>
  );
};
