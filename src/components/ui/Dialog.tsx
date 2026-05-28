import { Dialog as BaseDialog } from '@base-ui/react/dialog';
import type { ReactNode } from 'react';
import { cn } from '@/lib/util';

interface DialogProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: ReactNode;
}

interface DialogContentProps {
  title: string;
  description?: string;
  children: ReactNode;
  className?: string;
}

export const Dialog = ({
  open,
  onOpenChange,
  children,
}: DialogProps): React.JSX.Element => (
  <BaseDialog.Root open={open} onOpenChange={onOpenChange}>
    {children}
  </BaseDialog.Root>
);

export const DialogTrigger = BaseDialog.Trigger;

export const DialogContent = ({
  title,
  description,
  children,
  className,
}: DialogContentProps): React.JSX.Element => (
  <BaseDialog.Portal>
    <BaseDialog.Backdrop className="fixed inset-0 bg-foreground/30 backdrop-blur-[2px]" />
    <BaseDialog.Popup
      className={cn(
        'fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2',
        'w-full max-w-md bg-canvas-raised',
        'border border-border-muted',
        'shadow-[0_20px_60px_-10px_oklch(0%_0_0_/_0.20)]',
        'p-6 focus:outline-none',
        className,
      )}
    >
      <BaseDialog.Title className="text-sm font-semibold uppercase tracking-widest font-mono mb-1">
        {title}
      </BaseDialog.Title>
      {description && (
        <BaseDialog.Description className="text-xs text-muted-foreground font-mono mb-4">
          {description}
        </BaseDialog.Description>
      )}
      {children}
    </BaseDialog.Popup>
  </BaseDialog.Portal>
);

export const DialogClose = BaseDialog.Close;
