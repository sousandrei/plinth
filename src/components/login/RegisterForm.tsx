import { useMutation } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { renameSpace } from '@/api/spaces';
import { createUser, setPin } from '@/api/users';
import { cn } from '@/lib/util';
import type { User } from '@/types';
import { PinInput } from './PinInput';

interface RegisterFormProps {
  onCreated: (user: User) => void;
  onBack?: () => void;
  initialStep?: 'name' | 'pin' | 'confirm' | 'space';
}

export const RegisterForm = ({
  onCreated,
  onBack,
  initialStep = 'name',
}: RegisterFormProps): React.JSX.Element => {
  const [name, setName] = useState('');
  const [pin, setPin_] = useState('');
  const [spaceName, setSpaceName] = useState('');
  const [step, setStep] = useState<'name' | 'pin' | 'confirm' | 'space'>(
    initialStep,
  );
  const [nameError, setNameError] = useState('');
  const [pinError, setPinError] = useState('');
  const [spaceError, setSpaceError] = useState('');
  const nameRef = useRef<HTMLInputElement>(null);
  const spaceRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameRef.current?.focus();
  }, []);

  useEffect(() => {
    if (step === 'space') spaceRef.current?.focus();
  }, [step]);

  const createMutation = useMutation({
    mutationFn: async () => {
      const user = await createUser(name.trim());
      await setPin(user.id, pin);
      const defaultName = `${name.trim()}'s Space`;
      if (spaceName.trim() !== defaultName) {
        await renameSpace(spaceName.trim());
      }
      return user;
    },
    onSuccess: (user) => onCreated(user),
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
    setSpaceName(`${name.trim()}'s Space`);
    setStep('space');
  };

  const submitSpace = (e: React.FormEvent) => {
    e.preventDefault();
    if (!spaceName.trim()) {
      setSpaceError('Space name is required');
      return;
    }
    setSpaceError('');
    createMutation.mutate();
  };

  return (
    <div className="flex flex-col items-center gap-8 animate-slide-in-right">
      <div className="text-center">
        <h1 className="text-lg font-semibold tracking-tight">
          {step === 'space' ? 'Name your space' : 'Set up your profile'}
        </h1>
        <p className="text-xs font-mono text-muted-foreground mt-1">
          {step === 'name' && 'What should we call you?'}
          {step === 'pin' && 'Choose a 4-digit PIN to secure your data.'}
          {step === 'confirm' && 'Confirm your PIN.'}
          {step === 'space' && 'This is where your finances live.'}
        </p>
      </div>

      {step === 'name' && (
        <form
          onSubmit={submitName}
          className="flex flex-col gap-3 w-64 animate-fade-in"
        >
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
          {onBack && (
            <button
              type="button"
              onClick={onBack}
              className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-150 underline underline-offset-4 mt-2"
            >
              Back
            </button>
          )}
        </form>
      )}

      {step === 'pin' && (
        <div className="flex flex-col items-center gap-4 animate-slide-in-right">
          <PinInput
            onComplete={handlePinComplete}
            error={pinError || undefined}
            key="set-pin"
          />
          <button
            type="button"
            onClick={() => {
              setPinError('');
              setStep('name');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-150 underline underline-offset-4"
          >
            Back to Name
          </button>
        </div>
      )}

      {step === 'confirm' && (
        <div className="flex flex-col items-center gap-4 animate-slide-in-right">
          <PinInput
            onComplete={handleConfirmComplete}
            error={undefined}
            disabled={false}
            key="confirm-pin"
          />
          <button
            type="button"
            onClick={() => {
              setPin_('');
              setPinError('');
              setStep('pin');
            }}
            className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-150 underline underline-offset-4"
          >
            Back to Choose PIN
          </button>
        </div>
      )}

      {step === 'space' && (
        <form
          onSubmit={submitSpace}
          className="flex flex-col gap-3 w-64 animate-slide-in-right"
        >
          <input
            ref={spaceRef}
            value={spaceName}
            onChange={(e) => setSpaceName(e.target.value)}
            placeholder="My Space"
            className={cn(
              'w-full px-4 py-2.5 text-sm font-mono',
              'bg-canvas border border-border-muted',
              'placeholder:text-muted-foreground',
              'focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
              'transition-all duration-150',
              spaceError && 'border-expense',
            )}
          />
          {spaceError && (
            <p className="text-xs font-mono text-expense">{spaceError}</p>
          )}
          {createMutation.error && (
            <p className="text-xs font-mono text-expense">
              {String(createMutation.error)}
            </p>
          )}
          <button
            type="submit"
            disabled={createMutation.isPending}
            className={cn(
              'w-full px-4 py-2.5 text-xs font-mono uppercase tracking-widest',
              'bg-foreground text-canvas',
              'transition-all duration-150 active:scale-[0.98] hover:opacity-90',
              'disabled:opacity-50 disabled:pointer-events-none',
            )}
          >
            {createMutation.isPending ? 'Setting up…' : 'Get started'}
          </button>
        </form>
      )}
    </div>
  );
};
