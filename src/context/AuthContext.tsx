import type { ReactNode } from 'react';
import { createContext, useContext, useState } from 'react';
import type { User } from '@/types';

interface AuthState {
  user: User | null;
  spaceId: string | null;
  setUser: (user: User | null) => void;
  setSpaceId: (spaceId: string | null) => void;
}

const AuthContext = createContext<AuthState | null>(null);

interface AuthProviderProps {
  children: ReactNode;
}

export const AuthProvider = ({
  children,
}: AuthProviderProps): React.JSX.Element => {
  const [user, setUser] = useState<User | null>(null);
  const [spaceId, setSpaceId] = useState<string | null>(null);

  return (
    <AuthContext.Provider value={{ user, spaceId, setUser, setSpaceId }}>
      {children}
    </AuthContext.Provider>
  );
};

export const useAuth = (): AuthState => {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used inside AuthProvider');
  return ctx;
};
