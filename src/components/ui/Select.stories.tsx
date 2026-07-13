import { useState } from 'react';
import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { Select } from './Select';

const options = [
  { value: 'sek', label: 'SEK - Swedish Krona' },
  { value: 'eur', label: 'EUR - Euro' },
  { value: 'usd', label: 'USD - US Dollar' },
  { value: 'gbp', label: 'GBP - British Pound' },
];

const meta = {
  title: 'UI/Select',
  component: Select,
  tags: ['autodocs'],
  args: { options },
} satisfies Meta<typeof Select>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => {
    const [value, setValue] = useState<string | undefined>(undefined);
    return (
      <Select
        value={value}
        onValueChange={(v) => setValue(v ?? undefined)}
        options={options}
      />
    );
  },
};

export const Preselected: Story = {
  render: () => {
    const [value, setValue] = useState<string | undefined>('sek');
    return (
      <Select
        value={value}
        onValueChange={(v) => setValue(v ?? undefined)}
        options={options}
      />
    );
  },
};

export const Disabled: Story = {
  render: () => (
    <Select value="sek" onValueChange={() => {}} options={options} disabled />
  ),
};

export const CustomPlaceholder: Story = {
  render: () => {
    const [value, setValue] = useState<string | undefined>(undefined);
    return (
      <Select
        value={value}
        onValueChange={(v) => setValue(v ?? undefined)}
        options={options}
        placeholder="Choose a currency…"
      />
    );
  },
};
