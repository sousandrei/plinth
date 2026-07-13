import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState } from 'react';
import type { Preview } from 'storybook-react-rsbuild';
import { clearMocks } from '@/lib/tauri-mock';
import { resetPluginMocks } from '@/lib/tauri-plugin-mocks';
import '../src/styles/index.css';

const preview: Preview = {
  parameters: {
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
    backgrounds: {
      default: 'canvas',
      values: [
        { name: 'canvas', value: 'oklch(98.5% 0.004 80)' },
        { name: 'canvas-raised', value: 'oklch(99.5% 0.002 80)' },
      ],
    },
    a11y: {
      test: 'todo',
    },
  },
  beforeEach: [
    () => {
      clearMocks();
      resetPluginMocks();
    },
  ],
  decorators: [
    (Story) => {
      const [queryClient] = useState(
        () =>
          new QueryClient({
            defaultOptions: {
              queries: { retry: false, staleTime: 0 },
              mutations: { retry: false },
            },
          }),
      );
      return (
        <QueryClientProvider client={queryClient}>
          <Story />
        </QueryClientProvider>
      );
    },
  ],
};

export default preview;
