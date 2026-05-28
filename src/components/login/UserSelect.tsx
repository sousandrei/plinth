import { cn } from '@/lib/util';
import type { User } from '@/types';

interface UserSelectProps {
  users: User[];
  onSelect: (user: User) => void;
}

export const UserSelect = ({
  users,
  onSelect,
}: UserSelectProps): React.JSX.Element => (
  <div className="flex flex-col items-center gap-6 animate-slide-in-right">
    <div className="text-center">
      <h1 className="text-lg font-semibold tracking-tight">Who are you?</h1>
      <p className="text-xs font-mono text-muted-foreground mt-1">
        Select your profile to continue.
      </p>
    </div>
    <div className="flex flex-col gap-2 w-64">
      {users.map((u, i) => (
        <button
          key={u.id}
          type="button"
          onClick={() => onSelect(u)}
          style={{ animationDelay: `${i * 40}ms` }}
          className={cn(
            'flex items-center gap-3 px-4 py-3',
            'bg-canvas-raised border border-border-subtle',
            'text-left text-sm font-mono',
            'transition-all duration-150',
            'hover:border-border-muted hover:bg-muted/40 hover:-translate-y-px',
            'active:scale-[0.99]',
            'animate-fade-in',
          )}
        >
          <span className="w-7 h-7 rounded-full bg-foreground text-canvas text-[10px] font-semibold flex items-center justify-center shrink-0 uppercase">
            {u.name.slice(0, 2)}
          </span>
          <span>{u.name}</span>
        </button>
      ))}
    </div>
  </div>
);
