import { useState } from 'react';
import type { Meta } from 'storybook-react-rsbuild';
import { Tabs } from './Tabs';

const meta = {
  title: 'UI/Tabs',
  component: Tabs.Root,
  tags: ['autodocs'],
} satisfies Meta<typeof Tabs.Root>;

export default meta;

export const Default = {
  render: () => {
    const [value, setValue] = useState('overview');
    return (
      <Tabs.Root value={value} onValueChange={setValue} className="w-96">
        <Tabs.List>
          <Tabs.Tab value="overview">Overview</Tabs.Tab>
          <Tabs.Tab value="transactions">Transactions</Tabs.Tab>
          <Tabs.Tab value="settings">Settings</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="overview">
          <p className="p-6 text-sm text-muted-foreground">Overview content.</p>
        </Tabs.Panel>
        <Tabs.Panel value="transactions">
          <p className="p-6 text-sm text-muted-foreground">
            Transactions content.
          </p>
        </Tabs.Panel>
        <Tabs.Panel value="settings">
          <p className="p-6 text-sm text-muted-foreground">Settings content.</p>
        </Tabs.Panel>
      </Tabs.Root>
    );
  },
};
