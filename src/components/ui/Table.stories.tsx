import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { DEMO_ACCOUNTS } from '@/demo/data';
import { Table, TableBody, TableHead, TableRow, Td, TdMono, Th } from './Table';

const meta = {
  title: 'UI/Table',
  component: Table,
  tags: ['autodocs'],
} satisfies Meta<typeof Table>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Table>
      <TableHead>
        <TableRow>
          <Th>Name</Th>
          <Th>Type</Th>
          <Th>Currency</Th>
          <Th>Source</Th>
        </TableRow>
      </TableHead>
      <TableBody>
        {DEMO_ACCOUNTS.map((acc) => (
          <TableRow key={acc.id}>
            <Td>{acc.name}</Td>
            <TdMono>{acc.account_type}</TdMono>
            <TdMono>{acc.currency}</TdMono>
            <TdMono>{acc.account_source}</TdMono>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  ),
};

export const Empty: Story = {
  render: () => (
    <Table>
      <TableHead>
        <TableRow>
          <Th>Name</Th>
          <Th>Type</Th>
        </TableRow>
      </TableHead>
      <TableBody>
        <TableRow>
          <Td colSpan={2}>
            <span className="text-muted-foreground">No accounts found.</span>
          </Td>
        </TableRow>
      </TableBody>
    </Table>
  ),
};
