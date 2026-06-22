import { useQueryClient } from '@tanstack/react-query';
import { createRootRoute, Outlet } from '@tanstack/react-router';
import { listen } from '@tauri-apps/api/event';
import { check } from '@tauri-apps/plugin-updater';
import { useEffect } from 'react';
import { evictSpace } from '@/api/spaces';
import { restartApp } from '@/api/updater';
import { LoginPage } from '@/components/login/LoginPage';
import { AppNav } from '@/components/shared/AppNav';
import { Toaster, toast } from '@/components/ui/Toast';
import { useAuth } from '@/context/AuthContext';

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout(): React.JSX.Element {
  const { user, spaceId, setSpaceId } = useAuth();
  const queryClient = useQueryClient();

  useEffect(() => {
    let unlistenEvicted: (() => void) | null = null;
    let unlistenDeleted: (() => void) | null = null;
    let unlistenApplied: (() => void) | null = null;

    listen<string>('sync://evicted', (event) => {
      const evictedSpaceId = event.payload;
      evictSpace(evictedSpaceId).then(() => {
        if (spaceId === evictedSpaceId) {
          setSpaceId(null);
        }
      });
    }).then((fn) => {
      unlistenEvicted = fn;
    });

    listen<string>('sync://space-deleted', (event) => {
      const deletedSpaceId = event.payload;
      if (spaceId === deletedSpaceId) {
        setSpaceId(null);
      }
    }).then((fn) => {
      unlistenDeleted = fn;
    });

    // Emitted by the Rust sync session after every successful Batch
    // or Snapshot apply. Invalidate every cache key the UI reads so
    // pages refresh without the user having to reload.
    listen<{ space_id: string; rows: number; snapshot: boolean }>(
      'sync://applied',
      () => {
        queryClient.invalidateQueries({ queryKey: ['transactions'] });
        queryClient.invalidateQueries({ queryKey: ['accounts'] });
        queryClient.invalidateQueries({ queryKey: ['aggregations'] });
        queryClient.invalidateQueries({ queryKey: ['categories'] });
        queryClient.invalidateQueries({ queryKey: ['account-summaries'] });
        queryClient.invalidateQueries({ queryKey: ['models'] });
        queryClient.invalidateQueries({ queryKey: ['training'] });
        queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
        queryClient.invalidateQueries({ queryKey: ['trusted-devices'] });
      },
    ).then((fn) => {
      unlistenApplied = fn;
    });

    return () => {
      unlistenEvicted?.();
      unlistenDeleted?.();
      unlistenApplied?.();
    };
  }, [spaceId, setSpaceId, queryClient]);

  useEffect(() => {
    void (async () => {
      try {
        const update = await check();
        if (!update) return;

        const toastId = toast.info(
          `v${update.version} available`,
          'A new version of Plinth is ready to install.',
          {
            duration: Number.POSITIVE_INFINITY,
            action: {
              label: 'Install',
              onClick: () => {
                toast.dismiss(toastId);
                const loadingId = toast.loading('Downloading update…');
                update
                  .downloadAndInstall()
                  .then(() => {
                    toast.dismiss(loadingId);
                    toast.success(
                      'Update installed',
                      'Relaunch Plinth to apply the new version.',
                      {
                        duration: Number.POSITIVE_INFINITY,
                        action: {
                          label: 'Relaunch',
                          onClick: () => void restartApp(),
                        },
                      },
                    );
                  })
                  .catch((err: unknown) => {
                    toast.dismiss(loadingId);
                    toast.error(
                      'Update failed',
                      err instanceof Error ? err.message : String(err),
                    );
                  });
              },
            },
          },
        );
      } catch {
        // silently swallow startup network errors
      }
    })();
  }, []);

  if (!user || !spaceId) {
    return (
      <>
        <LoginPage />
        <Toaster />
      </>
    );
  }

  return (
    <div className="min-h-screen text-foreground font-sans flex flex-col">
      <AppNav />
      <main className="flex-1">
        <Outlet />
      </main>
      <Toaster />
    </div>
  );
}
