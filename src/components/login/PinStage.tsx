import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { verifyPin } from '@/api/users';
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
  const [error, setError] = useState('');
  const [attempts, setAttempts] = useState(0);

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

  return (
    <div className="flex flex-col items-center gap-6 animate-slide-in-right">
      <div className="flex flex-col items-center gap-2">
        <div className="w-12 h-12 rounded-full bg-foreground text-canvas text-base font-semibold flex items-center justify-center uppercase">
          {user.name.slice(0, 2)}
        </div>
        <h1 className="text-lg font-semibold tracking-tight">{user.name}</h1>
        <p className="text-xs font-mono text-muted-foreground">
          Enter your PIN to continue.
        </p>
      </div>

      <PinInput
        onComplete={(pin) => {
          setError('');
          verifyMutation.mutate(pin);
        }}
        error={error || undefined}
        disabled={verifyMutation.isPending}
        key={attempts}
      />

      {onBack && (
        <button
          type="button"
          onClick={onBack}
          className="text-xs font-mono text-muted-foreground hover:text-foreground transition-colors duration-150 underline underline-offset-4"
        >
          Switch user
        </button>
      )}
    </div>
  );
};
