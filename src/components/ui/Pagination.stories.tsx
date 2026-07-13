import { useState } from 'react';
import type { Meta } from 'storybook-react-rsbuild';
import { Pagination } from './Pagination';

const meta = {
  title: 'UI/Pagination',
  component: Pagination,
  tags: ['autodocs'],
} satisfies Meta<typeof Pagination>;

export default meta;

export const Default = {
  render: () => {
    const [page, setPage] = useState(2);
    return <Pagination page={page} pageCount={8} onPageChange={setPage} />;
  },
};

export const FirstPage = {
  render: () => {
    const [page, setPage] = useState(0);
    return <Pagination page={page} pageCount={5} onPageChange={setPage} />;
  },
};

export const LastPage = {
  render: () => {
    const [page, setPage] = useState(4);
    return <Pagination page={page} pageCount={5} onPageChange={setPage} />;
  },
};

export const SinglePage = {
  render: () => <Pagination page={0} pageCount={1} onPageChange={() => {}} />,
};
