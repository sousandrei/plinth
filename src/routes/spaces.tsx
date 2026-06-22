import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { save } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import {
  createSpace,
  exportSpaceData,
  listMySpaces,
  setActiveSpace,
} from '@/api/spaces';
import { forceSyncNow } from '@/api/sync';
import { ImportDialog } from '@/components/spaces/ImportDialog';
import { JoinSpaceModal } from '@/components/spaces/JoinSpaceModal';
import { SpaceEditDialog } from '@/components/spaces/SpaceEditDialog';
import { Button } from '@/components/ui/Button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Input } from '@/components/ui/Input';
import { useAuth } from '@/context/AuthContext';
import { cn } from '@/lib/util';
import type { Space } from '@/types';

export const Route = createFileRoute('/spaces')({
  component: SpacesPage,
});

function SpacesPage(): React.JSX.Element {
  const { user, spaceId, setSpaceId } = useAuth();
  const queryClient = useQueryClient();
  const [editingSpace, setEditingSpace] = useState<Space | null>(null);
  const [newSpaceOpen, setNewSpaceOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState('');
  const [joinOpen, setJoinOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [exportMsg, setExportMsg] = useState('');
  const [exportErr, setExportErr] = useState('');

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
    const remaining = spaces.filter((s) => s.id !== editingSpace?.id);
    if (remaining.length > 0) {
      setActiveSpace(remaining[0].id).then(() => setSpaceId(remaining[0].id));
    } else {
      setSpaceId(null);
    }
  };

  const handleExport = async (space: Space) => {
    setExportMsg('');
    setExportErr('');
    const path = await save({
      defaultPath: `${space.name}-export.json`,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!path) return;
    try {
      const res = await exportSpaceData(space.id, path);
      setExportMsg(
        `Exported ${res.transactions} transactions, ${res.accounts} accounts, ${res.categories} categories.`,
      );
    } catch (e) {
      setExportErr(String(e));
    }
  };

  const syncNowMutation = useMutation({
    mutationFn: forceSyncNow,
    onSuccess: (peerCount) => {
      setExportMsg(
        peerCount > 0
          ? `Sync started with ${peerCount} peer${peerCount === 1 ? '' : 's'} on the network.`
          : 'No peers visible on the network right now.',
      );
    },
    onError: (e) => setExportErr(String(e)),
  });

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
          <Button
            variant="secondary"
            className="text-xs rounded-none h-9"
            disabled={syncNowMutation.isPending}
            onClick={() => syncNowMutation.mutate()}
          >
            {syncNowMutation.isPending ? 'Syncing…' : 'Sync Now'}
          </Button>

          <Dialog open={importOpen} onOpenChange={setImportOpen}>
            <DialogTrigger
              render={
                <Button
                  variant="secondary"
                  className="text-xs rounded-none h-9"
                >
                  Import
                </Button>
              }
            />
            <DialogContent
              title="Import Space Data"
              description="Import categories, accounts, and transactions from an export file."
            >
              <ImportDialog
                spaces={spaces}
                onClose={() => setImportOpen(false)}
              />
            </DialogContent>
          </Dialog>

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
                  onClick={() => handleExport(space)}
                  className="text-xs rounded-none h-8 px-3"
                >
                  Export
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
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

      {(exportMsg || exportErr) && (
        <div className="fixed bottom-6 right-6 max-w-sm animate-fade-in">
          <div
            className={cn(
              'px-4 py-3 border text-xs font-mono',
              exportErr
                ? 'border-expense/40 bg-expense/5 text-expense'
                : 'border-border-muted bg-canvas-raised text-foreground',
            )}
          >
            {exportErr || exportMsg}
            <button
              type="button"
              onClick={() => {
                setExportMsg('');
                setExportErr('');
              }}
              className="ml-3 text-muted-foreground hover:text-foreground"
            >
              ✕
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
