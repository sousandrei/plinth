import { skipToken, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { getSyncSpaceRevision } from '@/api/sync';

const synchronizedQueries = [
  'transactions',
  'accounts',
  'account-summaries',
  'categories',
  'aggregations',
  'approved-count',
  'training-samples',
] as const;

export const useSyncRefresh = (spaceId: string | null): void => {
  const query = useQueryClient();
  const { data: revision } = useQuery({
    queryKey: ['sync-space-revision', spaceId],
    queryFn: spaceId === null ? skipToken : () => getSyncSpaceRevision(spaceId),
    refetchInterval: 2000,
    refetchIntervalInBackground: true,
  });

  useEffect(() => {
    if (spaceId === null || revision === undefined) return;
    for (const key of synchronizedQueries) {
      void query.invalidateQueries({ queryKey: [key] });
    }
  }, [query, spaceId, revision]);
};
