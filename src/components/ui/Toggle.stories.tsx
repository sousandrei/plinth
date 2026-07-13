import { useState } from 'react';
import type { Meta } from 'storybook-react-rsbuild';
import { Toggle } from './Toggle';

const meta = {
  title: 'UI/Toggle',
  component: Toggle,
  tags: ['autodocs'],
} satisfies Meta<typeof Toggle>;

export default meta;

export const Default = {
  render: () => {
    const [value, setValue] = useState('income');
    return (
      <Toggle
        value={value}
        onValueChange={setValue}
        options={[
          { value: 'income', label: 'Income' },
          { value: 'expense', label: 'Expense' },
        ]}
      />
    );
  },
};

export const Disabled = {
  render: () => {
    const [value, setValue] = useState('income');
    return (
      <Toggle
        value={value}
        onValueChange={setValue}
        disabled
        options={[
          { value: 'income', label: 'Income' },
          { value: 'expense', label: 'Expense' },
        ]}
      />
    );
  },
};

export const ThreeOptions = {
  render: () => {
    const [value, setValue] = useState('all');
    return (
      <Toggle
        value={value}
        onValueChange={setValue}
        options={[
          { value: 'all', label: 'All' },
          { value: 'approved', label: 'Approved' },
          { value: 'unapproved', label: 'Unapproved' },
        ]}
      />
    );
  },
};
