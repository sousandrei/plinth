import { useState } from 'react';
import type { Meta } from 'storybook-react-rsbuild';
import { DatePicker } from './DatePicker';

const meta = {
  title: 'UI/DatePicker',
  component: DatePicker,
  tags: ['autodocs'],
} satisfies Meta<typeof DatePicker>;

export default meta;

export const Empty = {
  render: () => {
    const [value, setValue] = useState('');
    return <DatePicker value={value} onChange={setValue} />;
  },
};

export const Filled = {
  render: () => {
    const [value, setValue] = useState('2026-07-13');
    return <DatePicker value={value} onChange={setValue} />;
  },
};

export const WithPlaceholder = {
  render: () => {
    const [value, setValue] = useState('');
    return (
      <DatePicker value={value} onChange={setValue} placeholder="Pick a date" />
    );
  },
};
