import { Tabs as BaseTabs } from '@base-ui/react/tabs';
import type { ReactNode } from 'react';
import { cn } from '@/lib/util';

interface TabsProps {
  value: string;
  onValueChange: (value: string) => void;
  children: ReactNode;
  className?: string;
}

export const Tabs = {
  Root: ({
    value,
    onValueChange,
    children,
    className,
  }: TabsProps): React.JSX.Element => (
    <BaseTabs.Root
      value={value}
      onValueChange={onValueChange}
      className={cn('flex flex-col w-full h-full rounded-none', className)}
    >
      {children}
    </BaseTabs.Root>
  ),
  List: ({
    children,
    className,
  }: {
    children: ReactNode;
    className?: string;
  }): React.JSX.Element => (
    <BaseTabs.List
      className={cn(
        'flex items-center gap-1 border-b border-border-muted bg-canvas px-2 py-1',
        className,
      )}
    >
      {children}
    </BaseTabs.List>
  ),
  Tab: ({
    value,
    children,
    className,
  }: {
    value: string;
    children: ReactNode;
    className?: string;
  }): React.JSX.Element => (
    <BaseTabs.Tab
      value={value}
      className={cn(
        'px-4 py-2 text-sm font-medium transition-colors duration-150',
        'rounded-md select-none cursor-pointer outline-none',
        'text-muted-foreground hover:text-foreground hover:bg-muted',
        'data-[selected]:text-foreground data-[selected]:bg-muted',
        'data-[selected]:shadow-[inset_0_2px_0_0_var(--color-accent)]',
        'focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-1',
        className,
      )}
    >
      {children}
    </BaseTabs.Tab>
  ),
  Panel: ({
    value,
    children,
    className,
  }: {
    value: string;
    children: ReactNode;
    className?: string;
  }): React.JSX.Element => (
    <BaseTabs.Panel
      value={value}
      className={cn('flex-1 min-h-0 overflow-auto rounded-none', className)}
    >
      {children}
    </BaseTabs.Panel>
  ),
};
