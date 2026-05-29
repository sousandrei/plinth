import { createRootRoute, Outlet } from '@tanstack/react-router';
import { LoginPage } from '@/components/login/LoginPage';
import { AppNav } from '@/components/shared/AppNav';
import { useAuth } from '@/context/AuthContext';

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout(): React.JSX.Element {
  const { user, spaceId } = useAuth();

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
