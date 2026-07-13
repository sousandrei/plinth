import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { StorybookConfig } from 'storybook-react-rsbuild';

const __dirname_local = path.dirname(fileURLToPath(import.meta.url));

const config: StorybookConfig = {
  stories: ['../src/**/*.mdx', '../src/**/*.stories.@(js|jsx|mjs|ts|tsx)'],
  addons: [
    '@storybook/addon-a11y',
    '@storybook/addon-docs',
    '@storybook/addon-onboarding',
  ],
  framework: 'storybook-react-rsbuild',
  rsbuildFinal: (rsbuildConfig) => {
    rsbuildConfig.resolve ??= {};
    rsbuildConfig.resolve.alias ??= {};
    const alias = rsbuildConfig.resolve.alias as Record<string, string>;
    alias['@tauri-apps/api/core'] = path.resolve(
      __dirname_local,
      '../src/lib/tauri-mock.ts',
    );
    return rsbuildConfig;
  },
};
export default config;
