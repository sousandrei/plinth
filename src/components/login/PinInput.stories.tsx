import type { Meta } from 'storybook-react-rsbuild';
import { PinInput } from './PinInput';

const meta = {
  title: 'Login/PinInput',
  component: PinInput,
  tags: ['autodocs'],
} satisfies Meta<typeof PinInput>;

export default meta;

export const Default = {
  render: () => <PinInput onComplete={() => {}} />,
};

export const WithError = {
  render: () => <PinInput onComplete={() => {}} error="Incorrect PIN" />,
};

export const Disabled = {
  render: () => <PinInput onComplete={() => {}} disabled />,
};

export const SixDigits = {
  render: () => <PinInput length={6} onComplete={() => {}} />,
};
