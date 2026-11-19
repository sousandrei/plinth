import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { setActiveSpace } from '@/api/spaces';
import {
  createSyncInvitation,
  discoverSyncPeers,
  joinSharedSpace,
} from '@/api/sync';
import { Button } from '@/components/ui/Button';
import { Dialog, DialogContent } from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import { toast } from '@/components/ui/Toast';
import type { Space } from '@/types';

interface ShareSpaceDialogProps {
  space: Space;
  onClose: () => void;
}

export const ShareSpaceDialog = ({
  space,
  onClose,
}: ShareSpaceDialogProps): React.JSX.Element => {
  const invitation = useMutation({
    mutationFn: () => createSyncInvitation(space.id),
  });

  const copyInvitation = async () => {
    if (!invitation.data) return;
    try {
      await navigator.clipboard.writeText(invitation.data.code);
      toast.success('Invitation copied');
    } catch {
      toast.error(
        'Could not copy the invitation',
        'Select and copy the code from the field.',
      );
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        title={`Share ${space.name}`}
        description="Create an invitation for the device you want to let join this space."
      >
        <div className="flex flex-col gap-4">
          {invitation.data && (
            <>
              <div className="flex flex-col gap-2">
                <label htmlFor="share-invitation" className="text-sm">
                  Invitation code
                </label>
                <Input
                  id="share-invitation"
                  readOnly
                  value={invitation.data.code}
                  onFocus={(event) => event.currentTarget.select()}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                On the other device, choose Join shared space and paste the
                code. The invitation can be used once and expires after five
                minutes.
              </p>
              <p className="text-xs text-muted-foreground">
                Nearby name{' '}
                <span className="text-foreground">{invitation.data.name}</span>
                {' · '}Port {invitation.data.port}
              </p>
            </>
          )}
          {invitation.isError && (
            <p className="text-sm text-expense" role="alert">
              {String(invitation.error)}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <Button
              variant={invitation.data ? 'secondary' : 'primary'}
              disabled={invitation.isPending}
              onClick={() => invitation.mutate()}
            >
              {invitation.isPending
                ? 'Creating…'
                : invitation.data
                  ? 'New invitation'
                  : 'Create invitation'}
            </Button>
            {invitation.data && (
              <Button onClick={() => void copyInvitation()}>
                Copy invitation
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
};

interface JoinSharedSpaceDialogProps {
  onClose: () => void;
  onJoined: (spaceId: string) => void;
}

export const JoinSharedSpaceDialog = ({
  onClose,
  onJoined,
}: JoinSharedSpaceDialogProps): React.JSX.Element => {
  const query = useQueryClient();
  const [code, setCode] = useState('');
  const [address, setAddress] = useState('');
  const [installedSpace, setInstalledSpace] = useState<string>();
  const nearby = useQuery({
    queryKey: ['nearby-sync-peers'],
    queryFn: discoverSyncPeers,
    refetchInterval: 6000,
    retry: false,
  });
  const invitations =
    nearby.data?.filter((peer) => peer.name.startsWith('join-')) ?? [];
  const join = useMutation({
    mutationFn: async () => {
      const id =
        installedSpace ??
        (await joinSharedSpace(code.trim(), address.trim() || undefined));
      setInstalledSpace(id);
      void query.invalidateQueries({ queryKey: ['my-spaces'] });
      await setActiveSpace(id);
      return id;
    },
    onSuccess: (id) => {
      for (const key of [
        'accounts',
        'transactions',
        'categories',
        'aggregations',
        'space-members',
      ]) {
        void query.invalidateQueries({ queryKey: [key] });
      }
      onJoined(id);
      toast.success('Shared space joined');
    },
  });

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !join.isPending) onClose();
      }}
    >
      <DialogContent
        title="Join shared space"
        description="Paste the invitation from the device sharing this space."
      >
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if ((code.trim() || installedSpace) && !join.isPending)
              join.mutate();
          }}
        >
          {!installedSpace && (
            <>
              <div className="flex flex-col gap-2">
                <label htmlFor="join-invitation" className="text-sm">
                  Invitation code
                </label>
                <Input
                  id="join-invitation"
                  value={code}
                  onChange={(event) => setCode(event.target.value)}
                />
              </div>
              <div className="flex flex-col gap-2">
                <h3 className="text-sm">Nearby devices</h3>
                {invitations.length > 0 ? (
                  <div className="flex flex-wrap gap-2">
                    {invitations.map((peer) => (
                      <Button
                        key={peer.name}
                        type="button"
                        variant={
                          address === `mdns:${peer.name}`
                            ? 'primary'
                            : 'secondary'
                        }
                        aria-pressed={address === `mdns:${peer.name}`}
                        onClick={() => setAddress(`mdns:${peer.name}`)}
                      >
                        {peer.name}
                      </Button>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-muted-foreground" role="status">
                    {nearby.isFetching
                      ? 'Looking for nearby devices…'
                      : nearby.isError
                        ? 'Discovery unavailable. You can enter an address below.'
                        : 'No invitations found nearby.'}
                  </p>
                )}
              </div>
              <div className="flex flex-col gap-2">
                <label htmlFor="join-address" className="text-sm">
                  Host or IP address (optional)
                </label>
                <Input
                  id="join-address"
                  value={address}
                  onChange={(event) => setAddress(event.target.value)}
                />
              </div>
            </>
          )}
          {join.isError && (
            <p className="text-sm text-expense" role="alert">
              {installedSpace
                ? 'The space was added. Try opening it again.'
                : String(join.error)}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="secondary"
              disabled={join.isPending}
              onClick={onClose}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={join.isPending || (!code.trim() && !installedSpace)}
            >
              {join.isPending
                ? 'Joining…'
                : installedSpace
                  ? 'Open space'
                  : 'Join'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
};
