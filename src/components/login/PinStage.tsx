import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { setPin, verifyPin } from '@/api/users';
import type { User } from '@/types';
import { PinInput } from './PinInput';

interface PinStageProps {
  user: User;
  onBack?: () => void;
  onSuccess: (user: User) => void;
}

export const PinStage = ({
  user,
  onBack,
  onSuccess,
}: PinStageProps): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [error, setError] = useState('');
  const [attempts, setAttempts] = useState(0);
  const [pin, setPin_] = useState('');
  const [step, setStep] = useState<'enter' | 'set' | 'confirm'>(
    user.has_pin ? 'enter' : 'set',
  );
  const [pinError, setPinError] = useState('');

  const verifyMutation = useMutation({
    mutationFn: (pin: string) => verifyPin(user.id, pin),
    onSuccess: (ok) => {
      if (ok) {
        onSuccess(user);
      } else {
        setAttempts((a) => a + 1);
        setError('Incorrect PIN');
      }
    },
    onError: (e) => setError(String(e)),
  });

  const setPinMutation = useMutation({
    mutationFn: async (p: string) => {
      await setPin(user.id, p);
      await verifyPin(user.id, p);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      onSuccess(user);
    },
    onError: (e) => setPinError(String(e)),
  });

  const handlePinComplete = (p: string) => {
    if (step === 'enter') {
      setError('');
      verifyMutation.mutate(p);
    } else if (step === 'set') {
      setPin_(p);
      setStep('confirm');
      setPinError('');
    } else {
      if (p !== pin) {
        setPinError("PINs don't match — try again");
        setStep('set');
        setPin_('');
        return;
      }
      setPinError('');
      setPinMutation.mutate(pin);
    }
  };

  const isSetFlow = !user.has_pin;

  return (
    <div className="flex flex-col items-center gap-6 animate-slide-in-right">
      <div className="flex flex-col items-center gap-2">
        <div className="w-12 h-12 rounded-full bg-foreground text-canvas text-base font-semibold flex items-center justify-center uppercase">
          {user.name.slice(0, 2)}
        </div>
        <h1 className="text-lg font-semibold tracking-tight">{user.name}</h1>
        <p className="text-xs font-mono text-muted-foreground">
          {isSetFlow
            ? step === 'set'
              ? 'Choose a 4-digit PIN to secure your data.'
              : 'Confirm your PIN.'
            : 'Enter your PIN to continue.'}
        </p>
      </div>

      <PinInput
        onComplete={handlePinComplete}
        error={isSetFlow ? pinError || undefined : error || undefined}
        disabled={
          isSetFlow
            ? step === 'confirm'
              ? false
              : setPinMutation.isPending
            : verifyMutation.isPending
        }
        key={isSetFlow ? `${step}-${attempts}` : attempts}
      />

      {isSetFlow && step === 'set' && (
        <button
          type="button"
          onClick={() => {
            setPin_('');
            setPinError('');
            setStep('enter');
          }}
          className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-150 underline underline-offset-4"
        >
          Back
        </button>
      )}

      {isSetFlow && step === 'confirm' && (
        <button
          type="button"
          onClick={() => {
            setPin_('');
            setPinError('');
            setStep('set');
          }}
          className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-150 underline underline-offset-4"
        >
          Back
        </button>
      )}

      {onBack && !isSetFlow && (
        <button
          type="button"
          onClick={onBack}
          className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-150 underline underline-offset-4"
        >
          Back
        </button>
      )}
    </div>
  );
};
