import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import {
  addSpaceMember,
  deleteSpace,
  leaveSpace,
  listSpaceMembers,
  removeSpaceMember,
} from '@/api/spaces';
import { listUsers } from '@/api/users';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Select } from '@/components/ui/Select';
import { useAuth } from '@/context/AuthContext';
import { cn } from '@/lib/util';

const MEMBERS_KEY = ['space-members'];

export function SpaceCard(): React.JSX.Element {
  const { spaceId, setSpaceId } = useAuth();
  const queryClient = useQueryClient();

  const { data: members = [], isLoading } = useQuery({
    queryKey: MEMBERS_KEY,
    queryFn: listSpaceMembers,
    enabled: !!spaceId,
  });

  const { data: allUsers = [] } = useQuery({
    queryKey: ['users'],
    queryFn: listUsers,
  });

  // Find the actual current user from the context.
  const { user } = useAuth();
  const currentMember = members.find((m) => m.user_id === user?.id);
  const isOwner = currentMember?.role === 'owner';

  // Add member: pick from users who aren't already members.
  const memberIds = new Set(members.map((m) => m.user_id));
  const addableUsers = allUsers.filter((u) => !memberIds.has(u.id));
  const [selectedUserId, setSelectedUserId] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [leaveOpen, setLeaveOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const addMutation = useMutation({
    mutationFn: () => {
      if (!selectedUserId) return Promise.reject(new Error('No user selected'));
      return addSpaceMember(selectedUserId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: MEMBERS_KEY });
      setSelectedUserId(null);
      setAddOpen(false);
    },
  });

  const removeMutation = useMutation({
    mutationFn: (userId: string) => removeSpaceMember(userId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: MEMBERS_KEY }),
  });

  const leaveMutation = useMutation({
    mutationFn: leaveSpace,
    onSuccess: () => {
      setSpaceId(null);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: deleteSpace,
    onSuccess: () => {
      setSpaceId(null);
    },
  });

  const addableOptions = addableUsers.map((u) => ({
    value: u.id,
    label: u.name,
  }));

  return (
    <Card>
      <CardHeader label="Space" />
      <CardBody className="space-y-5">
        {isLoading ? (
          <p className="text-xs font-mono text-muted-foreground">Loading…</p>
        ) : (
          <>
            {/* Members list */}
            <div className="flex flex-col gap-2">
              {members.map((member) => (
                <div
                  key={member.user_id}
                  className="flex items-center justify-between py-2 border-b border-border-subtle last:border-0"
                >
                  <div className="flex items-center gap-3">
                    <div className="w-7 h-7 rounded-full bg-foreground text-canvas text-[10px] font-semibold flex items-center justify-center uppercase shrink-0">
                      {member.name.slice(0, 2)}
                    </div>
                    <div className="flex flex-col">
                      <span className="text-sm font-medium">{member.name}</span>
                      <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">
                        {member.role}
                      </span>
                    </div>
                  </div>
                  {isOwner && member.user_id !== user?.id && (
                    <Button
                      variant="secondary"
                      onClick={() => removeMutation.mutate(member.user_id)}
                      disabled={removeMutation.isPending}
                      className="text-[10px] font-mono uppercase tracking-widest px-3 h-7 rounded-none border-border-muted text-muted-foreground hover:text-expense hover:border-expense/40"
                    >
                      Remove
                    </Button>
                  )}
                </div>
              ))}
            </div>

            {/* Add member (owner only) */}
            {isOwner && addableUsers.length > 0 && (
              <Dialog open={addOpen} onOpenChange={setAddOpen}>
                <DialogTrigger
                  render={
                    <Button
                      variant="secondary"
                      className="text-xs font-mono uppercase tracking-widest rounded-none h-9"
                    >
                      Add Member
                    </Button>
                  }
                />
                <DialogContent
                  title="Add Member"
                  description="Add an existing user to this space."
                >
                  <div className="space-y-5">
                    <Select
                      options={addableOptions}
                      value={selectedUserId ?? undefined}
                      onValueChange={setSelectedUserId}
                      placeholder="Select user…"
                    />
                    {addMutation.isError && (
                      <p className="text-xs font-mono text-expense">
                        {String(addMutation.error)}
                      </p>
                    )}
                    <div className="flex justify-end gap-2 pt-3 border-t border-border-subtle">
                      <DialogClose
                        render={
                          <Button
                            variant="secondary"
                            className="px-4 py-2 text-xs rounded-none h-9"
                          >
                            Cancel
                          </Button>
                        }
                      />
                      <Button
                        onClick={() => addMutation.mutate()}
                        disabled={!selectedUserId || addMutation.isPending}
                        className="px-4 py-2 text-xs rounded-none h-9"
                      >
                        {addMutation.isPending ? 'Adding…' : 'Add'}
                      </Button>
                    </div>
                  </div>
                </DialogContent>
              </Dialog>
            )}

            {/* Leave space (non-owner) */}
            {!isOwner && (
              <Dialog open={leaveOpen} onOpenChange={setLeaveOpen}>
                <DialogTrigger
                  render={
                    <Button
                      variant="secondary"
                      className={cn(
                        'text-xs font-mono uppercase tracking-widest rounded-none h-9',
                        'text-expense border-expense/30 hover:border-expense hover:bg-expense/5',
                      )}
                    >
                      Leave Space
                    </Button>
                  }
                />
                <DialogContent
                  title="Leave Space"
                  description="You will lose access to all data in this space."
                >
                  <div className="flex justify-end gap-2 pt-3 border-t border-border-subtle">
                    <DialogClose
                      render={
                        <Button
                          variant="secondary"
                          className="px-4 py-2 text-xs rounded-none h-9"
                        >
                          Cancel
                        </Button>
                      }
                    />
                    <Button
                      onClick={() => {
                        setLeaveOpen(false);
                        leaveMutation.mutate();
                      }}
                      disabled={leaveMutation.isPending}
                      className="bg-expense hover:bg-expense/90 text-white border-expense px-4 py-2 text-xs rounded-none h-9"
                    >
                      {leaveMutation.isPending ? 'Leaving…' : 'Leave Space'}
                    </Button>
                  </div>
                </DialogContent>
              </Dialog>
            )}

            {/* Delete space (owner only) */}
            {isOwner && (
              <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
                <DialogTrigger
                  render={
                    <Button
                      variant="secondary"
                      className={cn(
                        'text-xs font-mono uppercase tracking-widest rounded-none h-9',
                        'text-expense border-expense/30 hover:border-expense hover:bg-expense/5',
                      )}
                    >
                      Delete Space
                    </Button>
                  }
                />
                <DialogContent
                  title="Delete Space"
                  description="This will permanently delete all data in this space including accounts, transactions, and categories."
                >
                  <div className="flex justify-end gap-2 pt-3 border-t border-border-subtle">
                    <DialogClose
                      render={
                        <Button
                          variant="secondary"
                          className="px-4 py-2 text-xs rounded-none h-9"
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
                      className="bg-expense hover:bg-expense/90 text-white border-expense px-4 py-2 text-xs rounded-none h-9"
                    >
                      {deleteMutation.isPending ? 'Deleting…' : 'Delete Space'}
                    </Button>
                  </div>
                </DialogContent>
              </Dialog>
            )}

            {(leaveMutation.isError || deleteMutation.isError) && (
              <p className="text-xs font-mono text-expense">
                {String(leaveMutation.error ?? deleteMutation.error)}
              </p>
            )}
          </>
        )}
      </CardBody>
    </Card>
  );
}
