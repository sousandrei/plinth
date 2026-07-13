import { useState } from 'react';
import { Button } from '@/components/ui/Button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';

interface DangerZoneCardProps {
  onReset: () => void;
  isResetPending: boolean;
}

interface DangerActionProps {
  title: string;
  description: string;
  confirmTitle: string;
  confirmDescription: string;
  confirmLabel: string;
  onConfirm: () => void;
  isPending: boolean;
}

function DangerAction({
  title,
  description,
  confirmTitle,
  confirmDescription,
  confirmLabel,
  onConfirm,
  isPending,
}: DangerActionProps): React.JSX.Element {
  const [open, setOpen] = useState(false);

  return (
    <div className="flex items-center justify-between gap-6 py-4 border-b border-border-subtle last:border-b-0">
      <div className="flex flex-col gap-1 min-w-0">
        <p className="text-sm font-medium text-foreground">{title}</p>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogTrigger
          render={
            <Button
              variant="secondary"
              className="shrink-0 text-expense border-expense/30 hover:border-expense hover:bg-expense/5"
            >
              {title}
            </Button>
          }
        />
        <DialogContent title={confirmTitle} description={confirmDescription}>
          <div className="space-y-5 text-left">
            <div className="p-4 bg-expense/5 border border-expense/20 text-xs leading-relaxed text-expense font-sans">
              {confirmDescription}
            </div>
            <div className="flex justify-end gap-2 pt-3 border-t border-border-subtle">
              <DialogClose
                render={<Button variant="secondary">Cancel</Button>}
              />
              <Button
                onClick={() => {
                  setOpen(false);
                  onConfirm();
                }}
                disabled={isPending}
                className="bg-expense hover:bg-expense/90 text-white border-expense"
              >
                {isPending ? 'Resetting…' : confirmLabel}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export function DangerZoneCard({
  onReset,
  isResetPending,
}: DangerZoneCardProps): React.JSX.Element {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-[10px] font-mono uppercase tracking-widest text-expense font-bold">
        Danger Zone
      </h2>
      <div className="border border-border-muted bg-canvas-raised px-6">
        <DangerAction
          title="Factory Reset"
          description="Permanently delete the local database — all transactions, accounts, and profiles. The app will restart."
          confirmTitle="Confirm Factory Reset"
          confirmDescription="This is an extremely destructive operation. Your database will be wiped completely, and Plinth will close and restart."
          confirmLabel="Permanently Delete & Restart"
          onConfirm={onReset}
          isPending={isResetPending}
        />
      </div>
    </section>
  );
}
