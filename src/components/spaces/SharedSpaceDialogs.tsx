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
  verifySyncPairingCode,
} from '@/api/sync';
import { PinInput } from '@/components/login/PinInput';
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
  const request = useQuery({
    queryKey: ['sync-invitation-request', invitation.data?.id],
    queryFn: () => getPairingRequest(invitation.data?.id ?? ''),
    enabled: Boolean(invitation.data),
    refetchInterval: (query) =>
      query.state.data?.code === null ? false : 1000,
    retry: false,
  });
  const pending = request.data;
  const verified = pending?.code === null;
  const close = () => {
    if (invitation.isPending) return;
    if (invitation.data && !verified) {
      void cancelSyncInvitation(invitation.data.id).catch(() => undefined);
    }
    onClose();
  };

  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent
        title={`Share ${space.name}`}
        description={
          verified ? undefined : 'Open Join shared space on the other device.'
        }
      >
        <div className="flex flex-col gap-4">
          {verified ? (
            <>
              <p role="status">
                Code accepted. Finish joining on {pending.device}.
              </p>
              <div className="flex justify-end">
                <Button onClick={close}>Done</Button>
              </div>
            </>
          ) : pending ? (
            <>
              <p>Enter this code on {pending.device}.</p>
              <p className="font-mono text-2xl tracking-widest">
                {pending.code}
              </p>
              <div className="flex justify-end">
                <Button variant="secondary" onClick={close}>
                  Cancel
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
          {(invitation.isError || request.isError) && (
            <p className="text-sm text-expense" role="alert">
              {String(invitation.error ?? request.error)}
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
  const verify = useMutation({
    mutationFn: ({ id, code }: { id: string; code: string }) =>
      verifySyncPairingCode(id, code),
  });
  const pairing = begin.data;
  const needsPin = needsProfile || pairing?.same_user;
  const needsName = needsProfile && !pairing?.same_user;
  const ready = Boolean(
    pairing &&
      verify.isSuccess &&
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
    verify.reset();
    begin.reset();
    setPin('');
    setConfirmation('');
  };
  const close = () => {
    if (begin.isPending || verify.isPending || join.isPending) return;
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
              {!installedSpace && !verify.isSuccess && (
                <fieldset className="flex flex-col gap-2">
                  <legend className="mb-3 text-sm">
                    Enter the six-digit code shown on {pairing.device}.
                  </legend>
                  <PinInput
                    length={6}
                    visible
                    disabled={verify.isPending}
                    error={verify.isError ? String(verify.error) : undefined}
                    onComplete={(code) =>
                      verify.mutate({ id: pairing.id, code })
                    }
                  />
                </fieldset>
              )}
              {!installedSpace && verify.isSuccess && (
                <>
                  <p role="status">Code accepted.</p>
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
                    disabled={verify.isPending || join.isPending}
                    onClick={reset}
                  >
                    Back
                  </Button>
                )}
                {(verify.isSuccess || installedSpace) && (
                  <Button
                    type="submit"
                    disabled={join.isPending || (!ready && !installedSpace)}
                  >
                    {join.isPending
                      ? 'Joining…'
                      : installedSpace
                        ? 'Open space'
                        : 'Join space'}
                  </Button>
                )}
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
                    onClick={() =>
                      begin.mutate({
                        address: `mdns:${peer.name}`,
                        candidates: peer.addresses,
                      })
                    }
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
                      begin.mutate({ address: address.trim() });
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
