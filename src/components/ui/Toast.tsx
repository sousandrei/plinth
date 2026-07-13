import {
  CheckCircle,
  Info,
  WarningCircle,
  XCircle,
} from '@phosphor-icons/react';
import type { ReactElement } from 'react';
import { Toaster as SonnerToaster, toast as sonnerToast } from 'sonner';
import { cn } from '@/lib/util';

type ToastAction = { label: string; onClick: () => void };
type ToastOpts = { action?: ToastAction; duration?: number };

const iconClass = 'shrink-0 self-center';
const iconSize = 18;
const iconWeight = 'regular';

const renderIcon = (
  variant: 'success' | 'error' | 'info' | 'warning',
): ReactElement => {
  const colorClass = {
    success: 'text-growth',
    error: 'text-expense',
    info: 'text-accent',
    warning: 'text-highlight',
  }[variant];
  const Icon = {
    success: CheckCircle,
    error: XCircle,
    info: Info,
    warning: WarningCircle,
  }[variant];
  return (
    <Icon
      size={iconSize}
      weight={iconWeight}
      className={cn(iconClass, colorClass)}
    />
  );
};

export const toast = {
  success: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number =>
    sonnerToast.success(message, {
      description,
      ...opts,
      icon: renderIcon('success'),
    }),
  error: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number =>
    sonnerToast.error(message, {
      description,
      ...opts,
      icon: renderIcon('error'),
    }),
  info: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number =>
    sonnerToast.info(message, {
      description,
      ...opts,
      icon: renderIcon('info'),
    }),
  warning: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number =>
    sonnerToast.warning(message, {
      description,
      ...opts,
      icon: renderIcon('warning'),
    }),
  custom: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number => sonnerToast(message, { description, ...opts }),
  loading: (message: string): string | number =>
    sonnerToast.loading(message, { duration: Number.POSITIVE_INFINITY }),
  dismiss: (id?: string | number): void => {
    sonnerToast.dismiss(id);
  },
};

export function Toaster(): React.JSX.Element {
  return (
    <SonnerToaster
      position="bottom-right"
      closeButton
      toastOptions={{
        unstyled: true,
        classNames: {
          toast:
            'relative flex flex-row items-center gap-3 w-[360px] bg-canvas-raised border border-border-subtle pl-4 pr-12 py-3 shadow-[0_4px_12px_-2px_oklch(0%_0_0_/_0.08)] text-foreground font-sans focus:outline-none transition-all duration-200',
          icon: 'flex-shrink-0 self-center',
          content: 'flex-1 min-w-0 flex flex-col gap-0.5',
          title: 'text-sm font-semibold',
          description: 'text-xs text-muted-foreground leading-normal',
          actionButton:
            'text-xs font-medium border border-border-muted px-3 py-1.5 text-foreground hover:bg-muted transition-colors duration-150 cursor-pointer rounded-md',
          closeButton:
            'absolute right-3 top-1/2 -translate-y-1/2 bg-transparent border-0 shadow-none text-muted-foreground hover:text-foreground transition-colors duration-150 rounded-md w-7 h-7 p-0 flex items-center justify-center',
          success: '!border-l-4 !border-l-growth',
          error: '!border-l-4 !border-l-expense',
          info: '!border-l-4 !border-l-accent',
          warning: '!border-l-4 !border-l-highlight',
        },
      }}
    />
  );
}
