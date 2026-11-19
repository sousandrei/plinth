import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { setActiveSpace } from '@/api/spaces';
import {
  beginSyncPairing,
  cancelSyncInvitation,
  cancelSyncPairing,
  createSyncInvitation,
  discoverSyncPeers,
  getPairingRequest,
  joinSharedSpace,
  respondSyncPairing,
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
    mutationFn: (sameUser: boolean) => createSyncInvitation(space.id, sameUser),
  });
  const respond = useMutation({
    mutationFn: ({ id, approved }: { id: string; approved: boolean }) =>
      respondSyncPairing(id, approved),
    onSuccess: () => void request.refetch(),
  });
  const approved = respond.isSuccess && respond.variables.approved;
  const request = useQuery({
    queryKey: ['sync-invitation-request', invitation.data?.id],
    queryFn: () => getPairingRequest(invitation.data?.id ?? ''),
    enabled: Boolean(invitation.data) && !approved,
    refetchInterval: 1000,
    retry: false,
  });
  const close = () => {
    if (invitation.isPending || respond.isPending) return;
    if (invitation.data && !approved) {
      void cancelSyncInvitation(invitation.data.id).catch(() => undefined);
    }
    onClose();
  };
  const pending = request.data;

  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent
        title={`Share ${space.name}`}
        description="Open Join shared space on the other device."
      >
        <div className="flex flex-col gap-4">
          {approved ? (
            <p role="status">Approved. Finish joining on the other device.</p>
          ) : pending ? (
            <>
              <p>Does this code match the one on {pending.device}?</p>
              <p className="font-mono text-2xl tracking-widest">
                {pending.code}
              </p>
              <div className="flex justify-end gap-2">
                <Button
                  variant="secondary"
                  disabled={respond.isPending}
                  onClick={() =>
                    respond.mutate({ id: pending.id, approved: false })
                  }
                >
                  Reject
                </Button>
                <Button
                  disabled={respond.isPending}
                  onClick={() =>
                    respond.mutate({ id: pending.id, approved: true })
                  }
                >
                  Approve
                </Button>
              </div>
            </>
          ) : invitation.data ? (
            <>
              <p role="status">
                {invitation.data.nearby_available
                  ? `Select ${invitation.data.name} on the other device within five minutes.`
                  : 'Nearby advertising could not start. Connect by address within five minutes.'}
              </p>
              <div className="space-y-2 text-sm">
                <p className="text-muted-foreground">
                  If this device doesn’t appear, open{' '}
                  <strong>Join by address</strong> on the other device.
                </p>
                {invitation.data.addresses.length > 0 ? (
                  <ul aria-label="Local addresses" className="space-y-1">
                    {invitation.data.addresses.map((address) => (
                      <li key={address}>
                        <code className="select-all font-mono">{address}</code>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p role="status">
                    No local IPv4 address found. If you know the host address,
                    use port {invitation.data.port}.
                  </p>
                )}
              </div>
            </>
          ) : (
            <div className="flex flex-wrap gap-2">
              <Button
                disabled={invitation.isPending}
                onClick={() => invitation.mutate(true)}
              >
                Pair my device
              </Button>
              <Button
                variant="secondary"
                disabled={invitation.isPending}
                onClick={() => invitation.mutate(false)}
              >
                Invite someone else
              </Button>
            </div>
          )}
          {(invitation.isError || respond.isError || request.isError) && (
            <p className="text-sm text-expense" role="alert">
              {String(invitation.error ?? respond.error ?? request.error)}
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
};

interface JoinSharedSpaceDialogProps {
  needsProfile?: boolean;
  onClose: () => void;
  onJoined: (spaceId: string) => void;
}

export const JoinSharedSpaceDialog = ({
  needsProfile = false,
  onClose,
  onJoined,
}: JoinSharedSpaceDialogProps): React.JSX.Element => {
  const query = useQueryClient();
  const [address, setAddress] = useState('');
  const [name, setName] = useState('');
  const [pin, setPin] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [installedSpace, setInstalledSpace] = useState<string>();
  const begin = useMutation({ mutationFn: beginSyncPairing });
  const pairing = begin.data;
  const needsPin = needsProfile || pairing?.same_user;
  const needsName = needsProfile && !pairing?.same_user;
  const ready = Boolean(
    pairing &&
      (!needsPin ||
        (pin.length === 4 &&
          pin === confirmation &&
          (!needsName || name.trim()))),
  );
  const nearby = useQuery({
    queryKey: ['nearby-sync-peers'],
    queryFn: discoverSyncPeers,
    enabled: !pairing,
    refetchInterval: 6000,
    retry: false,
  });
  const invitations =
    nearby.data?.filter((peer) => peer.name.startsWith('join-')) ?? [];
  const join = useMutation({
    mutationFn: async () => {
      if (!pairing) throw new Error('Select a device first.');
      const id =
        installedSpace ??
        (await joinSharedSpace(
          pairing.id,
          needsPin
            ? { pin, name: needsName ? name.trim() : undefined }
            : undefined,
        ));
      setInstalledSpace(id);
      await setActiveSpace(id);
      return id;
    },
    onSuccess: (id) => {
      for (const key of [
        'my-spaces',
        'accounts',
        'transactions',
        'categories',
        'aggregations',
        'space-members',
      ]) {
        void query.invalidateQueries({ queryKey: [key] });
      }
      onJoined(id);
      void query.invalidateQueries({ queryKey: ['session'] });
      toast.success('Shared space joined');
    },
  });
  const reset = () => {
    if (pairing) void cancelSyncPairing(pairing.id).catch(() => undefined);
    join.reset();
    begin.reset();
    setPin('');
    setConfirmation('');
  };
  const close = () => {
    if (begin.isPending || join.isPending) return;
    reset();
    if (installedSpace) void query.invalidateQueries({ queryKey: ['session'] });
    onClose();
  };

  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent
        title="Join shared space"
        description="On the other device, open Spaces and choose Share space."
      >
        <div className="flex flex-col gap-4">
          {pairing ? (
            <form
              className="flex flex-col gap-4"
              onSubmit={(event) => {
                event.preventDefault();
                if ((ready || installedSpace) && !join.isPending) join.mutate();
              }}
            >
              {!installedSpace && (
                <>
                  <p>
                    Check that this code matches the one on {pairing.device}.
                  </p>
                  <p className="font-mono text-2xl tracking-widest">
                    {pairing.code}
                  </p>
                  {needsName && (
                    <label
                      htmlFor="join-profile-name"
                      className="flex flex-col gap-2 text-sm"
                    >
                      Your name
                      <Input
                        id="join-profile-name"
                        autoComplete="name"
                        value={name}
                        onChange={(event) => setName(event.target.value)}
                      />
                    </label>
                  )}
                  {needsPin && (
                    <>
                      <label
                        htmlFor="join-profile-pin"
                        className="flex flex-col gap-2 text-sm"
                      >
                        PIN for this device
                        <Input
                          type="password"
                          id="join-profile-pin"
                          inputMode="numeric"
                          autoComplete="new-password"
                          maxLength={4}
                          value={pin}
                          onChange={(event) =>
                            setPin(event.target.value.replace(/\D/g, ''))
                          }
                        />
                      </label>
                      <label
                        htmlFor="join-confirm-pin"
                        className="flex flex-col gap-2 text-sm"
                      >
                        Confirm PIN
                        <Input
                          type="password"
                          id="join-confirm-pin"
                          inputMode="numeric"
                          autoComplete="new-password"
                          maxLength={4}
                          value={confirmation}
                          onChange={(event) =>
                            setConfirmation(
                              event.target.value.replace(/\D/g, ''),
                            )
                          }
                        />
                      </label>
                      {confirmation.length === 4 && pin !== confirmation && (
                        <p className="text-sm text-expense" role="alert">
                          The PINs differ.
                        </p>
                      )}
                    </>
                  )}
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
                {!installedSpace && (
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={join.isPending}
                    onClick={reset}
                  >
                    Back
                  </Button>
                )}
                <Button
                  type="submit"
                  disabled={join.isPending || (!ready && !installedSpace)}
                >
                  {join.isPending
                    ? 'Joining…'
                    : installedSpace
                      ? 'Open space'
                      : 'Code matches, join'}
                </Button>
              </div>
            </form>
          ) : (
            <>
              <div className="flex flex-col gap-2">
                <h3 className="text-sm">Nearby devices</h3>
                {invitations.map((peer) => (
                  <Button
                    key={peer.name}
                    type="button"
                    variant="secondary"
                    disabled={begin.isPending}
                    onClick={() => begin.mutate(`mdns:${peer.name}`)}
                  >
                    {peer.display_name}
                  </Button>
                ))}
                {invitations.length === 0 && (
                  <p className="text-sm text-muted-foreground" role="status">
                    {nearby.isError
                      ? 'Nearby discovery is unavailable.'
                      : nearby.data
                        ? 'No nearby invitations found. Leave the sharing dialog open on the other device.'
                        : 'Looking for nearby devices…'}
                  </p>
                )}
              </div>
              <details className="text-sm">
                <summary className="cursor-pointer text-muted-foreground">
                  Join by address
                </summary>
                <form
                  className="mt-2 flex flex-col gap-2"
                  onSubmit={(event) => {
                    event.preventDefault();
                    if (address.trim() && !begin.isPending)
                      begin.mutate(address.trim());
                  }}
                >
                  <label
                    htmlFor="join-remote-address"
                    className="flex flex-col gap-2"
                  >
                    Host or IP address
                    <Input
                      id="join-remote-address"
                      value={address}
                      placeholder="host.example.com:37420"
                      onChange={(event) => setAddress(event.target.value)}
                    />
                  </label>
                  <Button
                    type="submit"
                    variant="secondary"
                    disabled={begin.isPending || !address.trim()}
                  >
                    Connect
                  </Button>
                </form>
              </details>
              {begin.isPending && <p role="status">Connecting…</p>}
              {begin.isError && (
                <p className="text-sm text-expense" role="alert">
                  {String(begin.error)}
                </p>
              )}
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
};
