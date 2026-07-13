import { useState } from 'react';
import type { Meta } from 'storybook-react-rsbuild';
import { Checkbox } from './Checkbox';

const meta = {
  title: 'UI/Checkbox',
  component: Checkbox,
  tags: ['autodocs'],
  argTypes: {
    checked: { control: 'boolean' },
    indeterminate: { control: 'boolean' },
    disabled: { control: 'boolean' },
  },
} satisfies Meta<typeof Checkbox>;

export default meta;

export const Unchecked = {
  render: () => {
    const [checked, setChecked] = useState(false);
    return (
      <Checkbox
        checked={checked}
        onCheckedChange={setChecked}
        label="Accept terms"
      />
    );
  },
};

export const Checked = {
  render: () => {
    const [checked, setChecked] = useState(true);
    return (
      <Checkbox
        checked={checked}
        onCheckedChange={setChecked}
        label="Accept terms"
      />
    );
  },
};

export const Indeterminate = {
  render: () => {
    const [checked, setChecked] = useState(false);
    return (
      <Checkbox
        checked={checked}
        indeterminate
        onCheckedChange={setChecked}
        label="Select all"
      />
    );
  },
};

export const Disabled = {
  render: () => {
    const [checked, setChecked] = useState(true);
    return (
      <Checkbox
        checked={checked}
        onCheckedChange={setChecked}
        disabled
        label="Locked option"
      />
    );
  },
};
