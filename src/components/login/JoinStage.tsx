import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { setActiveSpace } from '@/api/spaces';
import { getDeviceName, joinSpace, listPeers } from '@/api/sync';
import { createUser, setPin, verifyPin } from '@/api/users';
import { cn } from '@/lib/util';
import type { PeerInfo, User } from '@/types';
import { PinInput } from './PinInput';

type JoinStep = 'peer' | 'token' | 'pick' | 'name' | 'pin' | 'confirm';

interface BundleUser {
  id: string;
  name: string;
}

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
  const [token, setToken] = useState('');
  const [tokenError, setTokenError] = useState('');
  const [spaceId, setSpaceId] = useState('');
  const [bundleUsers, setBundleUsers] = useState<BundleUser[]>([]);
  const [pickedUser, setPickedUser] = useState<BundleUser | null>(null);
  const [newName, setNewName] = useState('');
  const [nameError, setNameError] = useState('');
  const [pin, setPin_] = useState('');
  const [pinError, setPinError] = useState('');
  const tokenRef = useRef<HTMLInputElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);

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

  useEffect(() => {
    if (step === 'token') tokenRef.current?.focus();
  }, [step]);

  const joinMutation = useMutation({
    mutationFn: () => {
      if (!selectedPeer) throw new Error('No peer selected');
      return joinSpace(selectedPeer.device_id, token.trim(), deviceName);
    },
    onSuccess: (result) => {
      setSpaceId(result.space_id);
      setBundleUsers(result.users);
      setStep('pick');
    },
    onError: (e) => setTokenError(String(e)),
  });

  const finishMutation = useMutation({
    mutationFn: async (pin: string): Promise<User> => {
      let user: User;
      if (pickedUser) {
        // Existing user from bundle — set the PIN then immediately verify it
        // so the backend session is established (verify_pin calls session.set).
        await setPin(pickedUser.id, pin);
        await verifyPin(pickedUser.id, pin);
        user = {
          id: pickedUser.id,
          name: pickedUser.name,
          has_pin: true,
          created_at: '',
          updated_at: '',
        };
      } else {
        // New person — create a fresh local user then add them to the space.
        user = await createUser(newName.trim());
        await setPin(user.id, pin);
      }
      await setActiveSpace(spaceId);
      return user;
    },
    onSuccess: (user) => {
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      onSuccess(user, spaceId);
    },
    onError: (e) => setPinError(String(e)),
  });

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
    finishMutation.mutate(p);
  };

  return (
    <div className="flex flex-col items-center gap-6 animate-slide-in-right">
      {/* Step 1: peer selection */}
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
                No devices found.
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
                    setStep('token');
                  }}
                  className={cn(
                    'flex items-center gap-3 px-4 py-3',
                    'bg-canvas-raised border border-border-subtle text-left text-sm font-mono',
                    'transition-all duration-150 hover:border-accent hover:bg-accent-muted/20 hover:-translate-y-px active:scale-[0.99]',
                  )}
                >
                  <span className="w-2 h-2 rounded-full bg-accent shrink-0" />
                  <span className="flex-1 truncate">
                    {peer.name || peer.host}
                  </span>
                  <span className="ml-auto text-[10px] text-muted-foreground">
                    {peer.host}
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

      {/* Step 2: token */}
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
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (!token.trim()) {
                setTokenError('Token is required');
                return;
              }
              setTokenError('');
              joinMutation.mutate();
            }}
            className="flex flex-col gap-3 w-64"
          >
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
                'bg-foreground text-canvas transition-all duration-150 active:scale-[0.98] hover:opacity-90',
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
              setStep('peer');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}

      {/* Step 3: pick user from bundle */}
      {step === 'pick' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">
              Who are you?
            </h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              Select your account from this space.
            </p>
          </div>
          <div className="flex flex-col gap-2 w-64">
            {bundleUsers.map((u) => (
              <button
                key={u.id}
                type="button"
                onClick={() => {
                  setPickedUser(u);
                  setStep('pin');
                }}
                className={cn(
                  'flex items-center gap-3 px-4 py-3',
                  'bg-canvas-raised border border-border-subtle text-left text-sm font-mono',
                  'transition-all duration-150 hover:border-accent hover:bg-accent-muted/20 hover:-translate-y-px active:scale-[0.99]',
                )}
              >
                <span className="w-7 h-7 rounded-full bg-foreground text-canvas text-[10px] font-semibold flex items-center justify-center uppercase shrink-0">
                  {u.name.slice(0, 2)}
                </span>
                <span>{u.name}</span>
              </button>
            ))}
            <button
              type="button"
              onClick={() => {
                setPickedUser(null);
                setStep('name');
              }}
              className={cn(
                'flex items-center gap-3 px-4 py-3',
                'border border-dashed border-border-muted text-left text-sm font-mono text-muted-foreground',
                'transition-all duration-150 hover:border-accent hover:text-foreground',
              )}
            >
              <span className="w-7 h-7 rounded-full border border-border-muted flex items-center justify-center text-xs shrink-0">
                +
              </span>
              <span>I'm someone new</span>
            </button>
          </div>
        </>
      )}

      {/* Step 3b: name input for new user */}
      {step === 'name' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">Your name</h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              How you'll appear in this space.
            </p>
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (!newName.trim()) {
                setNameError('Name is required');
                return;
              }
              setNameError('');
              setStep('pin');
            }}
            className="flex flex-col gap-3 w-64"
          >
            <input
              ref={nameRef}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Your name"
              className={cn(
                'w-full px-4 py-2.5 text-sm font-mono bg-canvas border border-border-muted',
                'placeholder:text-muted-foreground focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
                'transition-all duration-150',
                nameError && 'border-expense',
              )}
            />
            {nameError && (
              <p className="text-xs font-mono text-expense">{nameError}</p>
            )}
            <button
              type="submit"
              className="w-full px-4 py-2.5 text-xs font-mono uppercase tracking-widest bg-foreground text-canvas transition-all duration-150 active:scale-[0.98] hover:opacity-90"
            >
              Continue
            </button>
          </form>
          <button
            type="button"
            onClick={() => {
              setNameError('');
              setStep('pick');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}

      {/* Step 4: PIN */}
      {step === 'pin' && (
        <>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">
              {pickedUser ? `Set PIN for ${pickedUser.name}` : 'Choose a PIN'}
            </h1>
            <p className="text-xs font-mono text-muted-foreground mt-1">
              4-digit PIN to secure your data on this device.
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
              setStep(pickedUser ? 'pick' : 'name');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
          >
            Back
          </button>
        </>
      )}

      {/* Step 5: confirm PIN */}
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
          <PinInput
            onComplete={handleConfirmComplete}
            error={pinError || undefined}
            key="join-confirm"
            disabled={finishMutation.isPending}
          />
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
    </div>
  );
};
