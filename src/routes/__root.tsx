import { createRootRoute, Outlet } from '@tanstack/react-router';
import { listen } from '@tauri-apps/api/event';
import { useEffect } from 'react';
import { evictSpace } from '@/api/spaces';
import { LoginPage } from '@/components/login/LoginPage';
import { AppNav } from '@/components/shared/AppNav';
import { useAuth } from '@/context/AuthContext';

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout(): React.JSX.Element {
  const { user, spaceId, setSpaceId } = useAuth();

  useEffect(() => {
    let unlistenEvicted: (() => void) | null = null;
    let unlistenDeleted: (() => void) | null = null;

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

    return () => {
      unlistenEvicted?.();
      unlistenDeleted?.();
    };
  }, [spaceId, setSpaceId]);

  if (!user || !spaceId) {
    return <LoginPage />;
  }

  return (
    <div className="min-h-screen text-foreground font-sans flex flex-col">
      <AppNav />
      <main className="flex-1">
        <Outlet />
      </main>
    </div>
  );
}
