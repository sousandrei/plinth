import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import {
  getLocalDeviceId,
  listTrustedDevices,
  removeTrustedDevice,
} from '@/api/sync';
import { Button } from '@/components/ui/Button';
import { Dialog, DialogContent, DialogTrigger } from '@/components/ui/Dialog';
import type { TrustedDevice } from '@/types';
import { PairModal } from './PairModal';

interface DevicesSectionProps {
  spaceId: string;
}

export const DevicesSection = ({
  spaceId,
}: DevicesSectionProps): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [pairOpen, setPairOpen] = useState(false);

  const { data: localDeviceId } = useQuery({
    queryKey: ['local-device-id'],
    queryFn: getLocalDeviceId,
    staleTime: Infinity,
  });

  const { data: devices = [] } = useQuery({
    queryKey: ['trusted-devices', spaceId],
    queryFn: listTrustedDevices,
  });

  const removeMutation = useMutation({
    mutationFn: (id: string) => removeTrustedDevice(id),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ['trusted-devices', spaceId] }),
  });

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold">
          Devices
        </span>
        <Dialog open={pairOpen} onOpenChange={setPairOpen}>
          <DialogTrigger
            render={
              <Button
                variant="ghost"
                className="text-xs rounded-none h-7 px-2 text-muted-foreground hover:text-foreground"
              >
                + Pair Device
              </Button>
            }
          />
          <DialogContent
            title="Pair New Device"
            description="Pair another device to sync this space."
          >
            <PairModal onClose={() => setPairOpen(false)} />
          </DialogContent>
        </Dialog>
      </div>

      {devices.length === 0 ? (
        <p className="text-xs text-muted-foreground py-2">No paired devices.</p>
      ) : (
        <div className="flex flex-col divide-y divide-border-subtle border border-border-muted">
          {devices.map((device: TrustedDevice) => (
            <div
              key={device.id}
              className="flex items-center gap-3 px-3 py-2.5"
            >
              <div className="flex flex-col flex-1 min-w-0">
                <span className="text-sm truncate">{device.display_name}</span>
                <span className="text-[10px] font-mono text-muted-foreground">
                  Paired {device.paired_at.slice(0, 10)}
                </span>
              </div>
              {device.device_id !== localDeviceId ? (
                <Button
                  variant="ghost"
                  onClick={() => removeMutation.mutate(device.id)}
                  disabled={removeMutation.isPending}
                  className="px-2 h-8 text-xs text-muted-foreground hover:text-expense rounded-none shrink-0"
                >
                  Remove
                </Button>
              ) : (
                <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest shrink-0">
                  This device
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
