import { useMutation } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { factoryReset, updateUserName } from '@/api/users';
import { DangerZoneCard } from '@/components/settings/DangerZoneCard';
import { ProfileSettingsCard } from '@/components/settings/ProfileSettingsCard';
import { useAuth } from '@/context/AuthContext';

export const Route = createFileRoute('/settings')({
  component: Settings,
});

function Settings(): React.JSX.Element {
  const { user, setUser } = useAuth();

  const updateMutation = useMutation({
    mutationFn: (newName: string) => {
      if (!user) return Promise.reject(new Error('No active user'));
      return updateUserName(user.id, newName);
    },
    onSuccess: (updatedUser) => {
      setUser(updatedUser);
      alert('Profile updated successfully!');
    },
    onError: (err: unknown) => {
      alert(
        `Update failed: ${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  const resetMutation = useMutation({
    mutationFn: factoryReset,
    onError: (err: unknown) => {
      alert(
        `Factory reset failed: ${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  return (
    <div className="max-w-3xl mx-auto px-6 py-10 flex flex-col gap-8">
      <div className="flex flex-col gap-1 animate-fade-in text-left">
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="text-xs font-mono text-muted-foreground uppercase tracking-widest">
          Manage your account and app data
        </p>
      </div>

      <div className="flex flex-col gap-8 animate-fade-in">
        <ProfileSettingsCard
          user={user}
          onSave={(newName) => updateMutation.mutate(newName)}
          isPending={updateMutation.isPending}
        />

        <DangerZoneCard
          onReset={() => resetMutation.mutate()}
          isPending={resetMutation.isPending}
        />
      </div>
    </div>
  );
}
