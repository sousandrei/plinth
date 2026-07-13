import type { Meta } from 'storybook-react-rsbuild';
import { MiniLmSetupBanner } from './MiniLmSetupBanner';

const meta = {
  title: 'Training/MiniLmSetupBanner',
  component: MiniLmSetupBanner,
  tags: ['autodocs'],
} satisfies Meta<typeof MiniLmSetupBanner>;

export default meta;

export const Idle = {
  render: () => (
    <div className="w-lg">
      <MiniLmSetupBanner initialNeeded initialStatus="idle" />
    </div>
  ),
};

export const Downloading = {
  render: () => (
    <div className="w-lg">
      <MiniLmSetupBanner
        initialNeeded
        initialStatus="downloading"
        initialProgress={{ file: 'model.safetensors', step: 3, total: 8 }}
      />
    </div>
  ),
};

export const Done = {
  render: () => (
    <div className="w-lg">
      <MiniLmSetupBanner initialNeeded initialStatus="done" />
    </div>
  ),
};

export const WithError = {
  render: () => (
    <div className="w-lg">
      <MiniLmSetupBanner
        initialNeeded
        initialStatus="error"
        initialError="Network error: failed to connect to huggingface.co"
      />
    </div>
  ),
};
