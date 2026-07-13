import { useState } from 'react';
import type { Meta } from 'storybook-react-rsbuild';
import { Switch } from './Switch';

const meta = {
  title: 'UI/Switch',
  component: Switch,
  tags: ['autodocs'],
} satisfies Meta<typeof Switch>;

export default meta;

export const Off = {
  render: () => {
    const [checked, setChecked] = useState(false);
    return (
      <Switch
        checked={checked}
        onCheckedChange={setChecked}
        label="Notifications"
      />
    );
  },
};

export const On = {
  render: () => {
    const [checked, setChecked] = useState(true);
    return (
      <Switch
        checked={checked}
        onCheckedChange={setChecked}
        label="Notifications"
      />
    );
  },
};

export const Disabled = {
  render: () => {
    const [checked, setChecked] = useState(true);
    return (
      <Switch
        checked={checked}
        onCheckedChange={setChecked}
        disabled
        label="Locked setting"
      />
    );
  },
};
