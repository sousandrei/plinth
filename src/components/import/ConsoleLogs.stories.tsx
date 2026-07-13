import type { Meta } from 'storybook-react-rsbuild';
import { ConsoleLogs } from './ConsoleLogs';

const logs = [
  { id: '1', text: 'Parsing statement file...' },
  { id: '2', text: 'Found 12 rows' },
  { id: '3', text: 'Mapped account: 1234-5678' },
  { id: '4', text: 'Done. Output 11 transactions + 1 summary.' },
];

const meta = {
  title: 'Import/ConsoleLogs',
  component: ConsoleLogs,
  tags: ['autodocs'],
} satisfies Meta<typeof ConsoleLogs>;

export default meta;

export const Default = {
  render: () => (
    <ConsoleLogs logsList={logs} logsHeight={200} draggingPanel={null} />
  ),
};

export const Empty = {
  render: () => (
    <ConsoleLogs logsList={[]} logsHeight={200} draggingPanel={null} />
  ),
};

export const Dragging = {
  render: () => (
    <ConsoleLogs logsList={logs} logsHeight={200} draggingPanel="logs" />
  ),
};
