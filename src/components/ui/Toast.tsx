import { Toaster as SonnerToaster, toast as sonnerToast } from 'sonner';

type ToastAction = { label: string; onClick: () => void };
type ToastOpts = { action?: ToastAction; duration?: number };

export const toast = {
  success: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number => sonnerToast.success(message, { description, ...opts }),
  error: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number => sonnerToast.error(message, { description, ...opts }),
  info: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number => sonnerToast.info(message, { description, ...opts }),
  warning: (
    message: string,
    description?: string,
    opts?: ToastOpts,
  ): string | number => sonnerToast.warning(message, { description, ...opts }),
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
            'relative flex flex-row items-center gap-3 w-[360px] bg-canvas-raised border border-border-subtle px-4 py-3 pr-10 shadow-[0_4px_12px_-2px_oklch(0%_0_0_/_0.08)] text-foreground font-sans focus:outline-none transition-all duration-200',
          icon: 'flex-shrink-0 self-center',
          content: 'flex-1 min-w-0 flex flex-col gap-0.5',
          title: 'text-[10px] font-mono uppercase tracking-widest font-bold',
          description: 'text-xs text-muted-foreground leading-normal',
          actionButton:
            'text-[9px] font-mono uppercase tracking-widest border border-border-muted px-2.5 py-1 text-foreground hover:bg-muted transition-colors duration-150 cursor-pointer rounded-none',
          closeButton:
            '!absolute !right-3 !top-1/2 !-translate-y-1/2 !bg-transparent !border-0 !shadow-none !text-muted-foreground hover:!text-foreground !transition-colors !duration-150 !rounded-none !w-8 !h-8 !p-0',
          success: '!border-l-4 !border-l-growth',
          error: '!border-l-4 !border-l-expense',
          info: '!border-l-4 !border-l-accent',
          warning: '!border-l-4 !border-l-highlight',
        },
      }}
    />
  );
}
