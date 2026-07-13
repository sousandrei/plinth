import { useMutation } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { factoryReset, updateUserName } from '@/api/users';
import { DangerZoneCard } from '@/components/settings/DangerZoneCard';
import { ProfileSettingsCard } from '@/components/settings/ProfileSettingsCard';
import { UpdateSettingsCard } from '@/components/settings/UpdateSettingsCard';
import { Switch } from '@/components/ui/Switch';
import { toast } from '@/components/ui/Toast';
import { useAuth } from '@/context/AuthContext';
import { useDemoMode } from '@/hooks/useDemoMode';

export const Route = createFileRoute('/settings')({
  component: Settings,
});

function Settings(): React.JSX.Element {
  const { user, setUser } = useAuth();
  const { isDemoMode, toggle: toggleDemoMode } = useDemoMode();

  const updateMutation = useMutation({
    mutationFn: (newName: string) => {
      if (!user) return Promise.reject(new Error('No active user'));
      return updateUserName(user.id, newName);
    },
    onSuccess: (updatedUser) => {
      setUser(updatedUser);
      toast.success('Profile updated');
    },
    onError: (err: unknown) => {
      toast.error(
        'Update failed',
        err instanceof Error ? err.message : String(err),
      );
    },
  });

  const resetMutation = useMutation({
    mutationFn: factoryReset,
    onError: (err: unknown) => {
      toast.error(
        'Factory reset failed',
        err instanceof Error ? err.message : String(err),
      );
    },
  });

  return (
    <div className="max-w-3xl mx-auto px-6 py-10 flex flex-col gap-8">
      <div className="flex items-start justify-between animate-fade-in text-left">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
          <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
            Manage your account and app data
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-8 animate-fade-in">
        <ProfileSettingsCard
          user={user}
          onSave={(newName) => updateMutation.mutate(newName)}
          isPending={updateMutation.isPending}
        />

        {/* Demo Mode */}
        <div className="border border-border-muted bg-canvas-raised">
          <div className="px-6 py-4 border-b border-border-subtle">
            <span className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground">
              Demo Mode
            </span>
          </div>
          <div className="px-6 py-5 flex items-center justify-between gap-6">
            <div className="flex flex-col gap-1">
              <p className="text-sm font-medium text-foreground">
                Show fake financial data
              </p>
              <p className="text-xs text-muted-foreground max-w-sm">
                Replaces all accounts, transactions, and charts with realistic
                demo data. Useful for sharing or screen-recording without
                exposing your real finances.
              </p>
            </div>
            <Switch
              id="demo-mode-toggle"
              checked={isDemoMode}
              onCheckedChange={() => toggleDemoMode()}
              label="Toggle demo mode"
            />
          </div>
        </div>

        <UpdateSettingsCard />

        <DangerZoneCard
          onReset={() => resetMutation.mutate()}
          isPending={resetMutation.isPending}
        />
      </div>
    </div>
  );
}
