import type { Meta, StoryObj } from 'storybook-react-rsbuild';
import { Button } from './Button';
import { Toaster, toast } from './Toast';

const meta = {
  title: 'UI/Toast',
  component: Toaster,
  tags: ['autodocs'],
} satisfies Meta<typeof Toaster>;

export default meta;

type Story = StoryObj<typeof meta>;

export const AllTypes: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Toaster />
      <Button
        variant="primary"
        onClick={() => toast.success('Saved', 'Changes applied successfully.')}
      >
        Success
      </Button>
      <Button
        variant="primary"
        onClick={() => toast.error('Upload failed', 'File too large.')}
      >
        Error
      </Button>
      <Button
        variant="primary"
        onClick={() => toast.info('Sync started', 'Fetching latest data.')}
      >
        Info
      </Button>
      <Button
        variant="primary"
        onClick={() =>
          toast.warning('Deprecated', 'This feature will be removed.')
        }
      >
        Warning
      </Button>
      <Button
        variant="ghost"
        onClick={() =>
          toast.custom('Custom toast', 'With a custom description.')
        }
      >
        Custom
      </Button>
    </div>
  ),
};

export const WithAction: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Toaster />
      <Button
        variant="primary"
        onClick={() =>
          toast.success('Item deleted', 'Undo within 10 seconds.', {
            action: { label: 'Undo', onClick: () => toast.info('Restored') },
          })
        }
      >
        Delete with undo
      </Button>
    </div>
  ),
};
