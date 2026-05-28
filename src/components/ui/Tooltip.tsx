import { Tooltip as BaseTooltip } from '@base-ui/react/tooltip';
import type { ReactNode } from 'react';
import { cn } from '@/lib/util';

interface TooltipProps {
  content: ReactNode;
  children: ReactNode;
  delay?: number;
}

export const Tooltip = ({
  content,
  children,
  delay = 300,
}: TooltipProps): React.JSX.Element => (
  <BaseTooltip.Provider delay={delay}>
    <BaseTooltip.Root>
      <BaseTooltip.Trigger render={<span />}>{children}</BaseTooltip.Trigger>
      <BaseTooltip.Portal>
        <BaseTooltip.Positioner>
          <BaseTooltip.Popup
            className={cn(
              'bg-foreground text-canvas px-2.5 py-1.5',
              'text-xs font-mono leading-snug',
              'shadow-[0_4px_12px_-2px_oklch(0%_0_0_/_0.25)]',
              'max-w-xs',
            )}
          >
            {content}
          </BaseTooltip.Popup>
        </BaseTooltip.Positioner>
      </BaseTooltip.Portal>
    </BaseTooltip.Root>
  </BaseTooltip.Provider>
);
