import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { createContext, useCallback, useContext } from 'react';
import { getSession, type SessionSnapshot } from '@/api/spaces';
import type { User } from '@/types';

const QUERY_KEY = ['session'] as const;

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
  const queryClient = useQueryClient();

  const { data } = useQuery({
    queryKey: QUERY_KEY,
    queryFn: () => getSession(),
    staleTime: Infinity,
    refetchOnWindowFocus: true,
  });

  const user = data?.user ?? null;
  const spaceId = data?.space_id ?? null;

  const setUser = useCallback(
    (next: User | null) => {
      queryClient.setQueryData<SessionSnapshot>(QUERY_KEY, (prev) => ({
        user: next,
        space_id: prev?.space_id ?? null,
      }));
    },
    [queryClient],
  );

  const setSpaceId = useCallback(
    (next: string | null) => {
      queryClient.setQueryData<SessionSnapshot>(QUERY_KEY, (prev) => ({
        user: prev?.user ?? null,
        space_id: next,
      }));
    },
    [queryClient],
  );

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
