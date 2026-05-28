import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Spinner } from '@/components/ui/Spinner';
import type { User } from '@/types';

interface ProfileSettingsCardProps {
  user: User | null;
  onSave: (newName: string) => void;
  isPending: boolean;
}

export function ProfileSettingsCard({
  user,
  onSave,
  isPending,
}: ProfileSettingsCardProps): React.JSX.Element {
  const [name, setName] = useState(user?.name ?? '');

  useEffect(() => {
    if (user?.name) {
      setName(user.name);
    }
  }, [user]);

  const handleSave = () => {
    if (!name.trim()) return;
    onSave(name.trim());
  };

  const isNameUnchanged = !name.trim() || name.trim() === user?.name;

  return (
    <Card>
      <CardHeader label="Profile Settings" />
      <CardBody className="space-y-6">
        <div className="space-y-1.5 text-left">
          <label
            htmlFor="profile-name"
            className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
          >
            Your Name
          </label>
          <Input
            id="profile-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Enter your name..."
            disabled={isPending}
          />
        </div>

        <div className="flex justify-end pt-2 border-t border-border-subtle">
          <Button
            variant="primary"
            onClick={handleSave}
            disabled={isPending || isNameUnchanged}
            className="px-6 rounded-none h-10"
          >
            {isPending ? <Spinner size="sm" /> : 'Save Profile'}
          </Button>
        </div>
      </CardBody>
    </Card>
  );
}
