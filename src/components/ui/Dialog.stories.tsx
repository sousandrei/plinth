import { useState } from 'react';
import type { Meta } from 'storybook-react-rsbuild';
import { Button } from './Button';
import { Dialog, DialogClose, DialogContent, DialogTrigger } from './Dialog';
import { Input } from './Input';

const meta = {
  title: 'UI/Dialog',
  component: Dialog,
  tags: ['autodocs'],
} satisfies Meta<typeof Dialog>;

export default meta;

export const Default = {
  render: () => {
    const [open, setOpen] = useState(false);
    return (
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogTrigger
          render={<Button variant="secondary">Open dialog</Button>}
        />
        <DialogContent
          title="Create account"
          description="Enter a name for the new account."
        >
          <div className="flex flex-col gap-4">
            <Input placeholder="Account name" />
            <div className="flex justify-end gap-2">
              <DialogClose render={<Button variant="ghost">Cancel</Button>} />
              <DialogClose render={<Button variant="primary">Create</Button>} />
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  },
};
