import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { listSyncDevices, removeSyncDevice } from '@/api/sync';
import { Button } from '@/components/ui/Button';
import { toast } from '@/components/ui/Toast';

interface SyncDevicesProps {
  spaceId: string;
  canManage: boolean;
}

export const SyncDevices = ({
  spaceId,
  canManage,
}: SyncDevicesProps): React.JSX.Element => {
  const client = useQueryClient();
  const queryKey = ['sync-devices', spaceId];
  const devices = useQuery({
    queryKey,
    queryFn: () => listSyncDevices(spaceId),
    refetchInterval: 4000,
  });
  const remove = useMutation({
    mutationFn: (id: string) => removeSyncDevice(spaceId, id),
    onSuccess: () => client.invalidateQueries({ queryKey }),
    onError: (error) => toast.error('Could not remove device', String(error)),
  });

  return (
    <section className="flex flex-col gap-2 border-t border-border-subtle pt-4">
      <h3 className="text-sm font-semibold">Devices</h3>
      {devices.isLoading && (
        <p className="text-xs text-muted-foreground">Loading devices…</p>
      )}
      {devices.isError && (
        <p className="text-xs text-expense" role="alert">
          Could not load paired devices.
        </p>
      )}
      {devices.isSuccess && devices.data.length === 0 && (
        <p className="text-xs text-muted-foreground">No devices paired.</p>
      )}
      {devices.data?.map((device) => (
        <div
          key={device.id}
          className="flex justify-between gap-3 border border-border-subtle px-3 py-2"
        >
          <div className="min-w-0">
            <p className="truncate text-sm">{device.name}</p>
            <p
              className="text-xs text-muted-foreground"
              title={device.error ?? undefined}
            >
              {device.error ||
              (device.last_success !== null &&
                Date.now() - device.last_success > 25_000)
                ? 'Waiting to reconnect'
                : device.last_success === null
                  ? 'Waiting for first sync'
                  : `Last synced ${new Date(device.last_success).toLocaleString()}`}
            </p>
          </div>
          {canManage && (
            <Button
              variant="ghost"
              aria-label={`Remove device ${device.name}`}
              disabled={remove.isPending}
              onClick={() => remove.mutate(device.id)}
            >
              Remove
            </Button>
          )}
        </div>
      ))}
    </section>
  );
};
