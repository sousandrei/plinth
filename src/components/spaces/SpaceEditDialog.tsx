import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import {
  addSpaceMember,
  deleteSpace,
  leaveSpace,
  listSpaceMembers,
  removeSpaceMember,
  renameSpace,
  updateMemberRole,
} from '@/api/spaces';
import { forceSyncNow } from '@/api/sync';
import { addAppUser, listUsers, removeUser } from '@/api/users';
import { Button } from '@/components/ui/Button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { toast } from '@/components/ui/Toast';
import { cn } from '@/lib/util';
import type { Space, SpaceMember } from '@/types';
import { DevicesSection } from './DevicesSection';

interface SpaceEditDialogProps {
  space: Space;
  currentUserId: string;
  onClose: () => void;
  onDeleted: () => void;
}

export const SpaceEditDialog = ({
  space,
  currentUserId,
  onClose,
  onDeleted,
}: SpaceEditDialogProps): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [nameValue, setNameValue] = useState(space.name);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [addUserId, setAddUserId] = useState<string | null>(null);
  const [addMode, setAddMode] = useState<'existing' | 'create'>('existing');
  const [newUserName, setNewUserName] = useState('');

  const { data: members = [] } = useQuery({
    queryKey: ['space-members', space.id],
    queryFn: listSpaceMembers,
  });

  const { data: allUsers = [] } = useQuery({
    queryKey: ['users'],
    queryFn: listUsers,
  });

  const { data: userCount = 0 } = useQuery({
    queryKey: ['user-count'],
    queryFn: async () => {
      const users = await listUsers();
      return users.length;
    },
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

  const removeFromAppMutation = useMutation({
    mutationFn: (userId: string) => removeUser(userId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['space-members', space.id] });
      queryClient.invalidateQueries({ queryKey: ['users'] });
      queryClient.invalidateQueries({ queryKey: ['user-count'] });
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
    },
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

  const createAndAddMutation = useMutation({
    mutationFn: async () => {
      if (!newUserName.trim())
        return Promise.reject(new Error('Name is required'));
      const user = await addAppUser(newUserName.trim());
      await addSpaceMember(user.id);
      return user;
    },
    onSuccess: () => {
      setNewUserName('');
      setAddMode('existing');
      queryClient.invalidateQueries({ queryKey: ['space-members', space.id] });
      queryClient.invalidateQueries({ queryKey: ['users'] });
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

  const syncNowMutation = useMutation({
    mutationFn: forceSyncNow,
    onSuccess: (summary) => {
      const { dialed, ok, failed } = summary;
      if (dialed === 0) {
        toast.warning(
          'No peers dialled',
          'No trusted devices are on the network right now.',
        );
        return;
      }
      if (failed.length === 0) {
        toast.success(
          'Sync complete',
          `Synced with ${ok} ${ok === 1 ? 'peer' : 'peers'}.`,
        );
        return;
      }
      if (ok === 0) {
        toast.error(
          'Sync failed',
          `${failed.map((f) => `${f.name}: ${f.error}`).join('; ')}`,
        );
        return;
      }
      const failedNames = failed.map((f) => f.name).join(', ');
      toast.warning(
        'Sync partial',
        `Synced with ${ok} of ${dialed}; ${failedNames} failed.`,
      );
    },
    onError: (e) => toast.error('Sync failed', String(e)),
  });

  const roleOptions = [
    { value: 'owner', label: 'Owner' },
    { value: 'member', label: 'Member' },
  ];

  return (
    <div className="flex flex-col gap-6">
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
                  <Button
                    variant="ghost"
                    onClick={() => removeFromAppMutation.mutate(member.user_id)}
                    disabled={
                      removeFromAppMutation.isPending ||
                      userCount <= 1 ||
                      member.user_id === currentUserId
                    }
                    className="px-2 h-8 text-xs text-muted-foreground hover:text-expense rounded-none shrink-0 opacity-60 hover:opacity-100"
                  >
                    Remove from app
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

      {isOwner && (
        <div className="flex flex-col gap-2">
          <label
            htmlFor="add-member-select"
            className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
          >
            Add Member
          </label>
          <div className="flex gap-1">
            <button
              type="button"
              onClick={() => setAddMode('existing')}
              className={cn(
                'px-3 py-1 text-[10px] font-mono uppercase tracking-widest border transition-colors duration-150',
                addMode === 'existing'
                  ? 'bg-foreground text-canvas border-foreground'
                  : 'bg-transparent text-muted-foreground border-border-subtle hover:border-foreground',
              )}
            >
              Existing User
            </button>
            <button
              type="button"
              onClick={() => setAddMode('create')}
              className={cn(
                'px-3 py-1 text-[10px] font-mono uppercase tracking-widest border transition-colors duration-150',
                addMode === 'create'
                  ? 'bg-foreground text-canvas border-foreground'
                  : 'bg-transparent text-muted-foreground border-border-subtle hover:border-foreground',
              )}
            >
              Create New User
            </button>
          </div>
          {addMode === 'existing' && addableUsers.length > 0 && (
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
          )}
          {addMode === 'create' && (
            <div className="flex flex-col gap-2">
              <div className="flex gap-2">
                <Input
                  value={newUserName}
                  onChange={(e) => setNewUserName(e.target.value)}
                  placeholder="User name…"
                  className="flex-1"
                />
                <Button
                  onClick={() => createAndAddMutation.mutate()}
                  disabled={
                    !newUserName.trim() || createAndAddMutation.isPending
                  }
                  className="px-4 text-xs rounded-none h-10 shrink-0"
                >
                  {createAndAddMutation.isPending
                    ? 'Creating…'
                    : 'Create & Add'}
                </Button>
              </div>
              {createAndAddMutation.error && (
                <p className="text-xs font-mono text-expense">
                  {String(createAndAddMutation.error)}
                </p>
              )}
            </div>
          )}
          {addMode === 'existing' && addableUsers.length === 0 && (
            <p className="text-xs font-mono text-muted-foreground">
              No other users in the app
            </p>
          )}
        </div>
      )}

      {isOwner && <DevicesSection spaceId={space.id} />}

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
                {deleteMutation.isError && (
                  <p className="text-xs font-mono text-expense w-full">
                    {String(deleteMutation.error)}
                  </p>
                )}
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
        <Button
          variant="secondary"
          onClick={() => syncNowMutation.mutate()}
          disabled={syncNowMutation.isPending}
          className="ml-auto text-xs rounded-none h-9"
        >
          {syncNowMutation.isPending ? 'Syncing…' : 'Sync Now'}
        </Button>
      </div>
    </div>
  );
};
