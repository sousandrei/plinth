import { useMutation, useQuery } from '@tanstack/react-query';
import { listMySpaces, setActiveSpace } from '@/api/spaces';
import { cn } from '@/lib/util';
import type { Space } from '@/types';

interface SpacePickerStageProps {
  onSuccess: (spaceId: string) => void;
}

export const SpacePickerStage = ({
  onSuccess,
}: SpacePickerStageProps): React.JSX.Element => {
  const { data: spaces = [], isLoading } = useQuery({
    queryKey: ['my-spaces'],
    queryFn: listMySpaces,
  });

  const activateMutation = useMutation({
    mutationFn: (space: Space) => setActiveSpace(space.id),
    onSuccess: (_data, space) => onSuccess(space.id),
  });

  return (
    <div className="flex flex-col items-center gap-6 animate-slide-in-right">
      <div className="flex flex-col items-center gap-2">
        <h1 className="text-lg font-semibold tracking-tight">Select a Space</h1>
        <p className="text-xs font-mono text-muted-foreground">
          Choose which space to open.
        </p>
      </div>

      {isLoading ? (
        <span className="text-xs font-mono text-muted-foreground">
          Loading…
        </span>
      ) : (
        <div className="flex flex-col gap-2 w-64">
          {spaces.map((space) => (
            <button
              key={space.id}
              type="button"
              disabled={activateMutation.isPending}
              onClick={() => activateMutation.mutate(space)}
              className={cn(
                'w-full px-4 py-3 text-left',
                'border border-border-muted bg-canvas',
                'hover:border-accent hover:bg-accent-muted/30',
                'transition-all duration-150 active:scale-[0.98]',
                'disabled:opacity-50 disabled:cursor-not-allowed',
              )}
            >
              <div className="flex items-center justify-between">
                <span className="text-sm font-semibold">{space.name}</span>
                <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">
                  {space.role}
                </span>
              </div>
            </button>
          ))}
        </div>
      )}

      {activateMutation.isError && (
        <p className="text-xs font-mono text-expense">
          {String(activateMutation.error)}
        </p>
      )}
    </div>
  );
};
