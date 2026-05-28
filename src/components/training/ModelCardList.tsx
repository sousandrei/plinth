import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { cn } from '@/lib/util';
import type { ModelCard } from '@/types';

interface ModelCardListProps {
  models: ModelCard[];
  isLoading: boolean;
  selectedVersion: number | null;
  onSelect: (version: number) => void;
  onActivate: (version: number) => void;
  onDelete: (version: number) => void;
}

function fmtDate(raw: string): string {
  if (raw === 'shipped') return 'shipped';
  const d = new Date(raw);
  if (Number.isNaN(d.getTime())) return raw;
  return d.toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });
}

function StatPill({
  label,
  value,
}: {
  label: string;
  value: string;
}): React.JSX.Element {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[9px] font-mono uppercase tracking-widest text-muted-foreground">
        {label}
      </span>
      <span className="text-xs font-mono text-foreground">{value}</span>
    </div>
  );
}

export function ModelCardList({
  models,
  isLoading,
  selectedVersion,
  onSelect,
  onActivate,
  onDelete,
}: ModelCardListProps): React.JSX.Element {
  return (
    <Card>
      <CardHeader label="Models" meta={isLoading ? '' : `${models.length}`} />
      <CardBody className="p-0">
        {isLoading ? (
          <div className="flex justify-center py-10">
            <Spinner size="sm" />
          </div>
        ) : models.length === 0 ? (
          <p className="text-xs font-mono text-muted-foreground px-6 py-8 text-center">
            No models found
          </p>
        ) : (
          <ul className="divide-y divide-border-subtle">
            {[...models].reverse().map((m) => {
              const isSelected = m.version === selectedVersion;
              return (
                <li
                  key={m.version}
                  className={cn(
                    'flex items-stretch transition-colors duration-150',
                    isSelected
                      ? 'bg-muted border-l-2 border-l-accent'
                      : 'hover:bg-muted/40',
                  )}
                >
                  {/* Selectable content area */}
                  <button
                    type="button"
                    onClick={() => onSelect(m.version)}
                    className="flex-1 text-left px-6 py-4 flex flex-col gap-2 min-w-0 focus:outline-none"
                  >
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-sm font-mono font-semibold text-foreground">
                        {m.is_base ? 'Base' : `v${m.version}`}
                      </span>
                      {m.is_active && <Badge variant="growth">Active</Badge>}
                      {m.is_base && <Badge variant="muted">Shipped</Badge>}
                      <span className="text-[10px] font-mono text-muted-foreground">
                        {fmtDate(m.trained_at)}
                      </span>
                    </div>

                    {!m.is_base && (
                      <div className="flex flex-wrap gap-4">
                        <StatPill
                          label="Samples"
                          value={String(m.samples_used)}
                        />
                        <StatPill label="Epochs" value={String(m.epochs)} />
                        <StatPill
                          label="Val Acc"
                          value={`${(m.val_accuracy * 100).toFixed(1)}%`}
                        />
                      </div>
                    )}
                  </button>

                  {/* Action buttons — not part of the selection target */}
                  {!m.is_active && (
                    <div className="flex flex-col justify-center gap-1 pr-3 py-3 shrink-0">
                      <Button
                        variant="secondary"
                        onClick={() => onActivate(m.version)}
                        className="text-[10px] px-2 h-7 rounded-none"
                      >
                        Activate
                      </Button>
                      {!m.is_base && (
                        <Button
                          variant="ghost"
                          onClick={() => onDelete(m.version)}
                          className="text-[10px] px-2 h-7 rounded-none text-expense hover:text-expense hover:border-expense/40"
                        >
                          Delete
                        </Button>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </CardBody>
    </Card>
  );
}
