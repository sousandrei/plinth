import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import {
  addSpaceMember,
  createSpace,
  deleteSpace,
  leaveSpace,
  listMySpaces,
  listSpaceMembers,
  removeSpaceMember,
  renameSpace,
  setActiveSpace,
  updateMemberRole,
} from '@/api/spaces';
import {
  acceptPairTokenFromPeer,
  generatePairToken,
  getDeviceName,
  getLocalAddress,
  getLocalDeviceId,
  listPeers,
  listTrustedDevices,
  removeTrustedDevice,
} from '@/api/sync';
import { listUsers } from '@/api/users';
import { Button } from '@/components/ui/Button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { useAuth } from '@/context/AuthContext';
import { cn } from '@/lib/util';
import type {
  PairToken,
  PeerInfo,
  Space,
  SpaceMember,
  TrustedDevice,
} from '@/types';

export const Route = createFileRoute('/spaces')({
  component: SpacesPage,
});

// ---------------------------------------------------------------------------
// Pair modal
// ---------------------------------------------------------------------------

interface PairModalProps {
  spaceId: string;
  onClose: () => void;
}

function PairModal({ onClose }: PairModalProps): React.JSX.Element {
  const [token, setToken] = useState<PairToken | null>(null);
  const [secondsLeft, setSecondsLeft] = useState(0);

  const { data: deviceName = 'This device' } = useQuery({
    queryKey: ['device-name'],
    queryFn: getDeviceName,
    staleTime: Infinity,
  });

  const { data: localAddress } = useQuery({
    queryKey: ['local-address'],
    queryFn: getLocalAddress,
    staleTime: Infinity,
  });

  const generateMutation = useMutation({
    mutationFn: () => generatePairToken(deviceName),
    onSuccess: (pt) => {
      setToken(pt);
      const remaining = Math.max(
        0,
        pt.expires_at_unix - Math.floor(Date.now() / 1000),
      );
      setSecondsLeft(remaining);
      const iv = setInterval(() => {
        setSecondsLeft((s) => {
          if (s <= 1) {
            clearInterval(iv);
            return 0;
          }
          return s - 1;
        });
      }, 1000);
    },
  });

  return (
    <div className="flex flex-col gap-4">
      {token === null ? (
        <>
          <p className="text-xs text-muted-foreground">
            Generate a token on this device. The other device will find this one
            automatically on the local network and use the token to
            authenticate.
          </p>
          {generateMutation.isError && (
            <p className="text-xs font-mono text-expense">
              {String(generateMutation.error)}
            </p>
          )}
          <div className="flex justify-end gap-2 pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Cancel
            </Button>
            <Button
              onClick={() => generateMutation.mutate()}
              disabled={generateMutation.isPending}
              className="px-4 text-xs rounded-none h-9"
            >
              {generateMutation.isPending ? 'Generating…' : 'Generate Token'}
            </Button>
          </div>
        </>
      ) : (
        <>
          <div className="flex flex-col items-center gap-3 py-4">
            <span className="text-4xl font-mono font-bold tracking-[0.25em]">
              {token.token}
            </span>
            <span
              className={cn(
                'text-xs font-mono',
                secondsLeft > 10 ? 'text-muted-foreground' : 'text-expense',
              )}
            >
              {secondsLeft > 0 ? `Expires in ${secondsLeft}s` : 'Expired'}
            </span>
            <span className="text-[10px] font-mono text-muted-foreground">
              {deviceName}
              {localAddress ? ` · ${localAddress}` : ''}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            On the other device, choose{' '}
            <span className="font-mono text-foreground">Join via pairing</span>,
            select this device from the network list, then enter this token.
          </p>
          <div className="flex justify-end pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Done
            </Button>
          </div>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Devices section
// ---------------------------------------------------------------------------

interface DevicesSectionProps {
  spaceId: string;
}

function DevicesSection({ spaceId }: DevicesSectionProps): React.JSX.Element {
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
            <PairModal spaceId={spaceId} onClose={() => setPairOpen(false)} />
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
}

// ---------------------------------------------------------------------------
// Space edit modal
// ---------------------------------------------------------------------------

interface SpaceEditDialogProps {
  space: Space;
  currentUserId: string;
  onClose: () => void;
  onDeleted: () => void;
}

function SpaceEditDialog({
  space,
  currentUserId,
  onClose,
  onDeleted,
}: SpaceEditDialogProps): React.JSX.Element {
  const queryClient = useQueryClient();
  const [nameValue, setNameValue] = useState(space.name);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [addUserId, setAddUserId] = useState<string | null>(null);

  const { data: members = [] } = useQuery({
    queryKey: ['space-members', space.id],
    queryFn: listSpaceMembers,
  });

  const { data: allUsers = [] } = useQuery({
    queryKey: ['users'],
    queryFn: listUsers,
  });

  const isOwner = space.role === 'owner';
  const memberIds = new Set(members.map((m) => m.user_id));
  const addableUsers = allUsers.filter((u) => !memberIds.has(u.id));

  const renameMutation = useMutation({
    mutationFn: () => renameSpace(nameValue.trim()),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      onClose();
    },
  });

  const roleMutation = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      updateMemberRole(userId, role),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ['space-members', space.id] }),
  });

  const removeMutation = useMutation({
    mutationFn: (userId: string) => removeSpaceMember(userId),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ['space-members', space.id] }),
  });

  const addMutation = useMutation({
    mutationFn: () => {
      if (!addUserId) return Promise.reject(new Error('No user selected'));
      return addSpaceMember(addUserId);
    },
    onSuccess: () => {
      setAddUserId(null);
      queryClient.invalidateQueries({ queryKey: ['space-members', space.id] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: deleteSpace,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      onDeleted();
    },
  });

  const leaveMutation = useMutation({
    mutationFn: leaveSpace,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      onDeleted();
    },
  });

  const roleOptions = [
    { value: 'owner', label: 'Owner' },
    { value: 'member', label: 'Member' },
  ];

  return (
    <div className="flex flex-col gap-6">
      {/* Space name */}
      {isOwner && (
        <div className="flex flex-col gap-2">
          <label
            htmlFor="space-name-input"
            className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
          >
            Space Name
          </label>
          <div className="flex gap-2">
            <Input
              id="space-name-input"
              value={nameValue}
              onChange={(e) => setNameValue(e.target.value)}
              className="flex-1"
            />
            <Button
              onClick={() => renameMutation.mutate()}
              disabled={
                renameMutation.isPending ||
                !nameValue.trim() ||
                nameValue.trim() === space.name
              }
              className="px-4 text-xs rounded-none h-10 shrink-0"
            >
              {renameMutation.isPending ? 'Saving…' : 'Save'}
            </Button>
          </div>
          {renameMutation.isError && (
            <p className="text-xs font-mono text-expense">
              {String(renameMutation.error)}
            </p>
          )}
        </div>
      )}

      {/* Members */}
      <div className="flex flex-col gap-1">
        <label
          htmlFor="members-list"
          className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
        >
          Members
        </label>
        <div
          id="members-list"
          className="flex flex-col divide-y divide-border-subtle border border-border-muted"
        >
          {members.map((member: SpaceMember) => (
            <div
              key={member.user_id}
              className="flex items-center gap-3 px-3 py-2.5"
            >
              <div className="w-7 h-7 rounded-full bg-foreground text-canvas text-[10px] font-semibold flex items-center justify-center uppercase shrink-0">
                {member.name.slice(0, 2)}
              </div>
              <span className="text-sm flex-1 truncate">{member.name}</span>
              {isOwner && member.user_id !== currentUserId ? (
                <>
                  <Select
                    options={roleOptions}
                    value={member.role}
                    onValueChange={(role) => {
                      if (role)
                        roleMutation.mutate({ userId: member.user_id, role });
                    }}
                    className="w-32 py-1.5 text-xs"
                  />
                  <Button
                    variant="ghost"
                    onClick={() => removeMutation.mutate(member.user_id)}
                    disabled={removeMutation.isPending}
                    className="px-2 h-8 text-xs text-muted-foreground hover:text-expense rounded-none shrink-0"
                  >
                    Remove
                  </Button>
                </>
              ) : (
                <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">
                  {member.role}
                </span>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Add member (owner only) */}
      {isOwner && addableUsers.length > 0 && (
        <div className="flex flex-col gap-2">
          <label
            htmlFor="add-member-select"
            className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
          >
            Add Member
          </label>
          <div className="flex gap-2">
            <Select
              options={addableUsers.map((u) => ({
                value: u.id,
                label: u.name,
              }))}
              value={addUserId ?? undefined}
              onValueChange={setAddUserId}
              placeholder="Select user…"
              className="flex-1"
            />
            <Button
              onClick={() => addMutation.mutate()}
              disabled={!addUserId || addMutation.isPending}
              className="px-4 text-xs rounded-none h-10 shrink-0"
            >
              {addMutation.isPending ? 'Adding…' : 'Add'}
            </Button>
          </div>
        </div>
      )}

      {/* Devices (owner only) */}
      {isOwner && <DevicesSection spaceId={space.id} />}

      {/* Danger zone */}
      <div className="border-t border-border-subtle pt-4 flex gap-2">
        {isOwner ? (
          <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
            <DialogTrigger
              render={
                <Button
                  variant="secondary"
                  className="text-xs rounded-none h-9 text-expense border-expense/30 hover:border-expense hover:bg-expense/5"
                >
                  Delete Space
                </Button>
              }
            />
            <DialogContent
              title="Delete Space"
              description="Permanently deletes all accounts, transactions, and categories in this space."
            >
              <div className="flex justify-end gap-2 pt-3 border-t border-border-subtle">
                <DialogClose
                  render={
                    <Button
                      variant="secondary"
                      className="px-4 text-xs rounded-none h-9"
                    >
                      Cancel
                    </Button>
                  }
                />
                <Button
                  onClick={() => {
                    setDeleteOpen(false);
                    deleteMutation.mutate();
                  }}
                  disabled={deleteMutation.isPending}
                  className="bg-expense hover:bg-expense/90 text-white border-expense px-4 text-xs rounded-none h-9"
                >
                  {deleteMutation.isPending ? 'Deleting…' : 'Delete'}
                </Button>
              </div>
            </DialogContent>
          </Dialog>
        ) : (
          <Button
            variant="secondary"
            onClick={() => leaveMutation.mutate()}
            disabled={leaveMutation.isPending}
            className="text-xs rounded-none h-9 text-expense border-expense/30 hover:border-expense hover:bg-expense/5"
          >
            {leaveMutation.isPending ? 'Leaving…' : 'Leave Space'}
          </Button>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Join space modal (logged-in user joining a remote space via pairing)
// ---------------------------------------------------------------------------

type JoinStep = 'peer' | 'token';

interface JoinSpaceModalProps {
  onClose: () => void;
  onJoined: (spaceId: string) => void;
}

function JoinSpaceModal({
  onClose,
  onJoined,
}: JoinSpaceModalProps): React.JSX.Element {
  const queryClient = useQueryClient();
  const [step, setStep] = useState<JoinStep>('peer');
  const [selectedPeer, setSelectedPeer] = useState<PeerInfo | null>(null);
  const [token, setToken] = useState('');
  const [tokenError, setTokenError] = useState('');

  const { data: deviceName = 'This device' } = useQuery({
    queryKey: ['device-name'],
    queryFn: getDeviceName,
    staleTime: Infinity,
  });

  const {
    data: peers = [],
    isLoading: peersLoading,
    refetch,
  } = useQuery({
    queryKey: ['peers'],
    queryFn: listPeers,
    refetchInterval: 3000,
  });

  const joinMutation = useMutation({
    mutationFn: () => {
      if (!selectedPeer) throw new Error('No peer selected');
      return acceptPairTokenFromPeer(
        selectedPeer.device_id,
        token.trim(),
        deviceName,
      );
    },
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      onJoined(result.space_id);
    },
    onError: (e) => setTokenError(String(e)),
  });

  const submitToken = (e: React.FormEvent) => {
    e.preventDefault();
    if (!token.trim()) {
      setTokenError('Token is required');
      return;
    }
    setTokenError('');
    joinMutation.mutate();
  };

  return (
    <div className="flex flex-col gap-5">
      {step === 'peer' && (
        <>
          <p className="text-xs text-muted-foreground">
            Select a device on your local network to join one of its spaces.
          </p>
          {peersLoading ? (
            <span className="text-xs font-mono text-muted-foreground">
              Scanning…
            </span>
          ) : peers.length === 0 ? (
            <div className="flex flex-col gap-2">
              <span className="text-xs font-mono text-muted-foreground">
                No devices found on this network.
              </span>
              <button
                type="button"
                onClick={() => refetch()}
                className="self-start text-xs font-mono text-muted-foreground hover:text-foreground underline underline-offset-4 transition-colors duration-150"
              >
                Retry
              </button>
            </div>
          ) : (
            <div className="flex flex-col divide-y divide-border-subtle border border-border-muted">
              {peers.map((peer) => (
                <button
                  key={peer.device_id}
                  type="button"
                  onClick={() => {
                    setSelectedPeer(peer);
                    setStep('token');
                  }}
                  className={cn(
                    'flex items-center gap-3 px-4 py-3 text-left text-sm font-mono',
                    'transition-all duration-150',
                    'hover:bg-accent-muted/20',
                  )}
                >
                  <span className="w-2 h-2 rounded-full bg-accent shrink-0" />
                  <span className="flex-1 truncate">
                    {peer.name || peer.host}
                  </span>
                  <span className="text-[10px] text-muted-foreground shrink-0">
                    {peer.host}
                  </span>
                </button>
              ))}
            </div>
          )}
          <div className="flex justify-end pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Cancel
            </Button>
          </div>
        </>
      )}

      {step === 'token' && (
        <>
          <p className="text-xs text-muted-foreground">
            On{' '}
            <span className="font-mono text-foreground">
              {selectedPeer?.host}
            </span>
            , open{' '}
            <span className="font-mono">
              Spaces → Edit → Devices → Pair Device
            </span>{' '}
            and generate a token.
          </p>
          <form onSubmit={submitToken} className="flex flex-col gap-3">
            <input
              // biome-ignore lint/a11y/noAutofocus: intentional — only input in step
              autoFocus
              value={token}
              onChange={(e) =>
                setToken(e.target.value.replace(/\D/g, '').slice(0, 6))
              }
              placeholder="000000"
              inputMode="numeric"
              maxLength={6}
              className={cn(
                'w-full px-4 py-2.5 text-center text-2xl font-mono tracking-[0.4em]',
                'bg-canvas border border-border-muted',
                'placeholder:text-muted-foreground placeholder:tracking-normal placeholder:text-sm',
                'focus:outline-none focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-muted)]',
                'transition-all duration-150',
                tokenError && 'border-expense',
              )}
            />
            {tokenError && (
              <p className="text-xs font-mono text-expense">{tokenError}</p>
            )}
            <div className="flex justify-between pt-2 border-t border-border-subtle">
              <Button
                variant="ghost"
                type="button"
                onClick={() => {
                  setToken('');
                  setTokenError('');
                  setStep('peer');
                }}
                className="px-3 text-xs rounded-none h-9"
              >
                ← Back
              </Button>
              <Button
                type="submit"
                disabled={token.length < 6 || joinMutation.isPending}
                className="px-4 text-xs rounded-none h-9"
              >
                {joinMutation.isPending ? 'Joining…' : 'Join Space'}
              </Button>
            </div>
          </form>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

function SpacesPage(): React.JSX.Element {
  const { user, spaceId, setSpaceId } = useAuth();
  const queryClient = useQueryClient();
  const [editingSpace, setEditingSpace] = useState<Space | null>(null);
  const [newSpaceOpen, setNewSpaceOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState('');
  const [joinOpen, setJoinOpen] = useState(false);

  const { data: spaces = [], isLoading } = useQuery({
    queryKey: ['my-spaces'],
    queryFn: listMySpaces,
  });

  const switchMutation = useMutation({
    mutationFn: (id: string) => setActiveSpace(id),
    onSuccess: (_data, id) => {
      setSpaceId(id);
      queryClient.invalidateQueries({ queryKey: ['space-members'] });
    },
  });

  const createMutation = useMutation({
    mutationFn: () => createSpace(newSpaceName.trim()),
    onSuccess: (space) => {
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      return setActiveSpace(space.id).then(() => {
        setSpaceId(space.id);
        setNewSpaceName('');
        setNewSpaceOpen(false);
      });
    },
  });

  const handleSpaceDeleted = () => {
    queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
    setEditingSpace(null);
    // If deleted the active space, pick the first remaining one or clear.
    const remaining = spaces.filter((s) => s.id !== editingSpace?.id);
    if (remaining.length > 0) {
      setActiveSpace(remaining[0].id).then(() => setSpaceId(remaining[0].id));
    } else {
      setSpaceId(null);
    }
  };

  return (
    <div className="max-w-[800px] mx-auto px-6 py-10 flex flex-col gap-8">
      <div className="flex items-center justify-between animate-fade-in">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold tracking-tight">Spaces</h1>
          <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
            {isLoading
              ? 'Loading…'
              : `${spaces.length} space${spaces.length !== 1 ? 's' : ''}`}
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Dialog open={joinOpen} onOpenChange={setJoinOpen}>
            <DialogTrigger
              render={
                <Button
                  variant="secondary"
                  className="text-xs rounded-none h-9"
                >
                  Join Space
                </Button>
              }
            />
            <DialogContent
              title="Join Space"
              description="Connect to another device on your local network."
            >
              <JoinSpaceModal
                onClose={() => setJoinOpen(false)}
                onJoined={(sid) => {
                  setActiveSpace(sid).then(() => {
                    setSpaceId(sid);
                    setJoinOpen(false);
                    queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
                  });
                }}
              />
            </DialogContent>
          </Dialog>

          <Dialog open={newSpaceOpen} onOpenChange={setNewSpaceOpen}>
            <DialogTrigger
              render={
                <Button className="text-xs rounded-none h-9">
                  + New Space
                </Button>
              }
            />
            <DialogContent
              title="Create Space"
              description="A new space starts empty — you can import accounts and transactions afterwards."
            >
              <div className="flex flex-col gap-4">
                <Input
                  placeholder="Space name"
                  value={newSpaceName}
                  onChange={(e) => setNewSpaceName(e.target.value)}
                  autoFocus
                />
                {createMutation.isError && (
                  <p className="text-xs font-mono text-expense">
                    {String(createMutation.error)}
                  </p>
                )}
                <div className="flex justify-end gap-2 pt-2 border-t border-border-subtle">
                  <DialogClose
                    render={
                      <Button
                        variant="secondary"
                        className="px-4 text-xs rounded-none h-9"
                      >
                        Cancel
                      </Button>
                    }
                  />
                  <Button
                    onClick={() => createMutation.mutate()}
                    disabled={!newSpaceName.trim() || createMutation.isPending}
                    className="px-4 text-xs rounded-none h-9"
                  >
                    {createMutation.isPending ? 'Creating…' : 'Create'}
                  </Button>
                </div>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </div>

      {/* Space list */}
      <div className="flex flex-col gap-3 animate-fade-in">
        {spaces.map((space) => {
          const isActive = space.id === spaceId;
          return (
            <div
              key={space.id}
              className={cn(
                'flex items-center justify-between px-5 py-4',
                'border border-border-muted bg-canvas-raised',
                isActive && 'border-accent/50 bg-accent-muted/10',
                'transition-colors duration-150',
              )}
            >
              <div className="flex items-center gap-4">
                {isActive && (
                  <div className="w-1.5 h-1.5 rounded-full bg-accent shrink-0" />
                )}
                <div className="flex flex-col gap-0.5">
                  <span className="text-sm font-semibold">{space.name}</span>
                  <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">
                    {space.role}
                  </span>
                </div>
              </div>

              <div className="flex items-center gap-2">
                {!isActive && (
                  <Button
                    variant="secondary"
                    onClick={() => switchMutation.mutate(space.id)}
                    disabled={switchMutation.isPending}
                    className="text-xs rounded-none h-8 px-3"
                  >
                    Switch
                  </Button>
                )}
                <Button
                  variant="ghost"
                  onClick={() => {
                    // Ensure we're editing from the active space context.
                    if (!isActive) {
                      setActiveSpace(space.id).then(() => {
                        setSpaceId(space.id);
                        setEditingSpace(space);
                      });
                    } else {
                      setEditingSpace(space);
                    }
                  }}
                  className="text-xs rounded-none h-8 px-3"
                >
                  Edit
                </Button>
              </div>
            </div>
          );
        })}
      </div>

      {/* Edit modal */}
      {editingSpace && (
        <Dialog
          open={!!editingSpace}
          onOpenChange={(open) => {
            if (!open) setEditingSpace(null);
          }}
        >
          <DialogContent
            title={editingSpace.name}
            description={`Manage members and settings for this space.`}
          >
            <SpaceEditDialog
              space={editingSpace}
              currentUserId={user?.id ?? ''}
              onClose={() => {
                setEditingSpace(null);
                queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
              }}
              onDeleted={handleSpaceDeleted}
            />
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}
