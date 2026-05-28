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
        'flex border-b border-border-muted bg-canvas w-full rounded-none',
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
        'px-5 py-2.5 text-[10px] font-mono uppercase tracking-[0.15em] transition-colors duration-100 font-bold',
        'border-r border-border-muted select-none cursor-pointer rounded-none outline-none',
        'text-muted-foreground hover:text-foreground hover:bg-canvas-raised',
        'data-[selected]:text-foreground data-[selected]:bg-canvas',
        'data-[selected]:border-b-2 data-[selected]:border-b-accent-primary',
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
