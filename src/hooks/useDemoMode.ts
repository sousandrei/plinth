import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { getAppSetting, setAppSetting } from '@/api/settings';

const DEMO_KEY = 'demo_mode';
const QUERY_KEY = ['app-setting', DEMO_KEY] as const;

export function useDemoMode(): {
  isDemoMode: boolean;
  isLoading: boolean;
  toggle: () => void;
} {
  const queryClient = useQueryClient();

  const { data, isLoading } = useQuery({
    queryKey: QUERY_KEY,
    queryFn: () => getAppSetting(DEMO_KEY),
    staleTime: Infinity,
  });

  const isDemoMode = data === '1';

  const mutation = useMutation({
    mutationFn: (next: boolean) => setAppSetting(DEMO_KEY, next ? '1' : '0'),
    onSuccess: (_data, next) => {
      queryClient.setQueryData(QUERY_KEY, next ? '1' : '0');
      // Invalidate all data queries so every page re-fetches with demo/real data
      queryClient.invalidateQueries({
        predicate: (q) => q.queryKey[0] !== 'app-setting',
      });
    },
  });

  return {
    isDemoMode,
    isLoading,
    toggle: () => mutation.mutate(!isDemoMode),
  };
}
