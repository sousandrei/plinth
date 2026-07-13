import type { Preview } from 'storybook-react-rsbuild';
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
  },
};

export default preview;
