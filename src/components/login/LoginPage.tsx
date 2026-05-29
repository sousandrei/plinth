import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { listMySpaces } from '@/api/spaces';
import { listUsers } from '@/api/users';
import { useAuth } from '@/context/AuthContext';
import type { User } from '@/types';
import { PinStage } from './PinStage';
import { RegisterForm } from './RegisterForm';
import { SpacePickerStage } from './SpacePickerStage';
import { UserSelect } from './UserSelect';

type Stage = 'loading' | 'register' | 'select' | 'pin' | 'space';
type Direction = 'forward' | 'back';

export const LoginPage = (): React.JSX.Element => {
  const { setUser, setSpaceId } = useAuth();
  const [stage, setStage] = useState<Stage>('loading');
  const [direction, setDirection] = useState<Direction>('forward');
  const [selectedUser, setSelectedUser] = useState<User | null>(null);

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: listUsers,
  });

  useEffect(() => {
    if (isLoading || !users) return;
    if (users.length === 0) setStage('register');
    else if (users.length === 1) {
      setSelectedUser(users[0]);
      setStage('pin');
    } else setStage('select');
  }, [users, isLoading]);

  const goToPin = (user: User) => {
    setDirection('forward');
    setSelectedUser(user);
    setStage('pin');
  };

  const goBack = () => {
    setDirection('back');
    setStage('select');
  };

  const handlePinSuccess = async (user: User) => {
    // After PIN, check how many spaces this user has.
    // The backend auto-selects when there is exactly one, so we only
    // need to show the picker when there are multiple.
    try {
      const spaces = await listMySpaces();
      if (spaces.length === 1) {
        // Backend already set the active space — mirror it in the frontend.
        setUser(user);
        setSpaceId(spaces[0].id);
      } else {
        // Multiple spaces: show the picker (session is set, no space yet).
        setSelectedUser(user);
        setDirection('forward');
        setStage('space');
      }
    } catch {
      // Fallback: enter the app anyway; commands will return Unauthorized if
      // something is wrong server-side.
      setUser(user);
    }
  };

  const handleSpaceSelected = (spaceId: string) => {
    if (!selectedUser) return;
    setSpaceId(spaceId);
    setUser(selectedUser);
  };

  const handleRegistered = async (user: User) => {
    // createUser auto-creates exactly one space and sets the full session.
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
      <div className="absolute top-8 left-1/2 -translate-x-1/2 flex items-center gap-2.5 animate-fade-in">
        <span className="text-sm font-semibold tracking-tight font-sans">
          Plinth
        </span>
        <span className="text-[10px] font-mono text-muted-foreground tracking-[0.2em] uppercase bg-muted px-1.5 py-0.5 border border-border-subtle">
          Wealth
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

        {stage === 'register' && <RegisterForm onCreated={handleRegistered} />}

        {stage === 'select' && users && (
          <div key="select" className={animClass}>
            <UserSelect users={users} onSelect={goToPin} />
          </div>
        )}

        {stage === 'pin' && selectedUser && (
          <div key={`pin-${selectedUser.id}`} className={animClass}>
            <PinStage
              user={selectedUser}
              onBack={users && users.length > 1 ? goBack : undefined}
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
