import { useQuery } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { listMySpaces, setActiveSpace } from '@/api/spaces';
import { listUsers } from '@/api/users';
import logoClean from '@/assets/logo-clean.svg';
import { JoinSharedSpaceDialog } from '@/components/spaces/SharedSpaceDialogs';
import { Button } from '@/components/ui/Button';
import { useAuth } from '@/context/AuthContext';
import type { User } from '@/types';
import { PinStage } from './PinStage';
import { RegisterForm } from './RegisterForm';
import { SpacePickerStage } from './SpacePickerStage';
import { UserSelect } from './UserSelect';

type Stage = 'loading' | 'welcome' | 'register' | 'select' | 'pin' | 'space';
type Direction = 'forward' | 'back';

export const LoginPage = (): React.JSX.Element => {
  const { setUser, setSpaceId } = useAuth();
  const [stage, setStage] = useState<Stage>('loading');
  const [direction, setDirection] = useState<Direction>('forward');
  const [selectedUser, setSelectedUser] = useState<User | null>(null);
  const [previousStage, setPreviousStage] = useState<Stage | null>(null);
  const [joinOpen, setJoinOpen] = useState(false);
  const initialized = useRef(false);

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: listUsers,
  });

  useEffect(() => {
    if (isLoading || !users) return;
    if (initialized.current) return;
    initialized.current = true;
    if (users.length === 0) setStage('welcome');
    else setStage('select');
  }, [users, isLoading]);

  const goToPin = (user: User) => {
    setDirection('forward');
    setSelectedUser(user);
    setStage('pin');
  };

  const goBack = () => {
    setDirection('back');
    setStage(previousStage || 'select');
  };

  const handleAddUser = () => {
    setDirection('forward');
    setPreviousStage(stage);
    setStage('register');
  };

  const handlePinSuccess = async (user: User) => {
    try {
      const spaces = await listMySpaces();
      if (spaces.length === 1) {
        await setActiveSpace(spaces[0].id);
        setUser(user);
        setSpaceId(spaces[0].id);
      } else {
        setSelectedUser(user);
        setDirection('forward');
        setStage('space');
      }
    } catch {
      setUser(user);
    }
  };

  const handleSpaceSelected = (spaceId: string) => {
    if (!selectedUser) return;
    setSpaceId(spaceId);
    setUser(selectedUser);
  };

  const handleRegistered = async (user: User) => {
    try {
      const spaces = await listMySpaces();
      setUser(user);
      setSpaceId(spaces[0]?.id ?? null);
    } catch {
      setUser(user);
    }
  };

  const animClass =
    direction === 'forward'
      ? 'animate-slide-in-right'
      : 'animate-slide-in-left';

  return (
    <div className="min-h-screen flex flex-col items-center justify-center bg-canvas">
      <div className="absolute top-8 left-1/2 -translate-x-1/2 flex flex-col items-center gap-2 animate-fade-in text-center">
        <img src={logoClean} className="w-8 h-8 shrink-0" alt="" />
        <span className="text-sm font-semibold tracking-tight font-sans">
          Plinth
        </span>
      </div>

      <div className="w-full max-w-sm px-6">
        {stage === 'loading' && (
          <div className="flex justify-center animate-fade-in">
            <span className="text-xs font-mono text-muted-foreground">
              Loading…
            </span>
          </div>
        )}

        {stage === 'welcome' && (
          <div className="flex flex-col items-center gap-8 animate-fade-in">
            <div className="text-center">
              <h1 className="text-lg font-semibold tracking-tight">
                Welcome to Plinth
              </h1>
              <p className="text-xs font-mono text-muted-foreground mt-1">
                Personal finance, local-first.
              </p>
            </div>
            <div className="flex flex-col gap-3 w-64">
              <button
                type="button"
                onClick={() => {
                  setDirection('forward');
                  setPreviousStage('welcome');
                  setStage('register');
                }}
                className="w-full px-4 py-3 text-sm font-mono bg-foreground text-canvas transition-all duration-150 active:scale-[0.98] hover:opacity-90"
              >
                Get started
              </button>
              <Button variant="secondary" onClick={() => setJoinOpen(true)}>
                Join shared space
              </Button>
              {joinOpen && (
                <JoinSharedSpaceDialog
                  needsProfile
                  onClose={() => setJoinOpen(false)}
                  onJoined={() => setJoinOpen(false)}
                />
              )}
            </div>
          </div>
        )}

        {stage === 'register' && (
          <div key="register" className={animClass}>
            <RegisterForm onCreated={handleRegistered} onBack={goBack} />
          </div>
        )}

        {stage === 'select' && users && (
          <div key="select" className={animClass}>
            <UserSelect
              users={users}
              onSelect={goToPin}
              onAdd={handleAddUser}
            />
          </div>
        )}

        {stage === 'pin' && selectedUser && (
          <div key={`pin-${selectedUser.id}`} className={animClass}>
            <PinStage
              user={selectedUser}
              onBack={goBack}
              onSuccess={handlePinSuccess}
            />
          </div>
        )}

        {stage === 'space' && (
          <div key="space" className={animClass}>
            <SpacePickerStage onSuccess={handleSpaceSelected} />
          </div>
        )}
      </div>
    </div>
  );
};
