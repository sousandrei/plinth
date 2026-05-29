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
import type { Space, SpaceMember } from '@/types';

export const Route = createFileRoute('/spaces')({
  component: SpacesPage,
});

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
// Main page
// ---------------------------------------------------------------------------

function SpacesPage(): React.JSX.Element {
  const { user, spaceId, setSpaceId } = useAuth();
  const queryClient = useQueryClient();
  const [editingSpace, setEditingSpace] = useState<Space | null>(null);
  const [newSpaceOpen, setNewSpaceOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState('');

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

        <Dialog open={newSpaceOpen} onOpenChange={setNewSpaceOpen}>
          <DialogTrigger
            render={
              <Button className="text-xs rounded-none h-9">+ New Space</Button>
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
