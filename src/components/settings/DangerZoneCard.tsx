import { useState } from 'react';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';

interface DangerZoneCardProps {
  onReset: () => void;
  isPending: boolean;
}

export function DangerZoneCard({
  onReset,
  isPending,
}: DangerZoneCardProps): React.JSX.Element {
  const [resetOpen, setResetOpen] = useState(false);

  return (
    <Card className="border-expense/20">
      <CardHeader label="Danger Zone" className="bg-expense/90 text-white" />
      <CardBody className="space-y-4">
        <p className="text-xs text-muted-foreground leading-relaxed font-sans text-left">
          Factory resetting will permanently delete the local SQLite database
          containing all your transactions, account data, and user profiles.
          This action is irreversible and will restart the application
          instantly.
        </p>

        <div className="pt-2">
          <Dialog open={resetOpen} onOpenChange={(val) => setResetOpen(val)}>
            <DialogTrigger
              render={
                <Button
                  variant="secondary"
                  className="w-full text-expense border-expense/30 hover:border-expense hover:bg-expense/5 text-xs font-mono uppercase tracking-widest rounded-none h-10"
                >
                  Factory Reset
                </Button>
              }
            />
            <DialogContent
              title="Confirm Factory Reset"
              description="This is an extremely destructive operation."
            >
              <div className="space-y-5 text-left">
                <div className="p-4 bg-expense/5 border border-expense/20 text-xs leading-relaxed text-expense font-sans">
                  You are about to delete all your application data. Your
                  database will be wiped completely, and Julius will close and
                  restart.
                </div>

                <div className="flex justify-end gap-2 pt-3 border-t border-border-subtle">
                  <DialogClose
                    render={
                      <Button
                        variant="secondary"
                        className="px-4 py-2 text-xs rounded-none h-9"
                      >
                        Cancel
                      </Button>
                    }
                  />
                  <Button
                    onClick={() => {
                      setResetOpen(false);
                      onReset();
                    }}
                    disabled={isPending}
                    className="bg-expense hover:bg-expense/90 text-white border-expense px-4 py-2 text-xs rounded-none h-9 cursor-pointer"
                  >
                    {isPending
                      ? 'Resetting...'
                      : 'Permanently Delete & Restart'}
                  </Button>
                </div>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </CardBody>
    </Card>
  );
}
