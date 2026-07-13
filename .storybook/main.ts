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
    alias['@tauri-apps/api/app'] = path.resolve(
      __dirname_local,
      '../src/lib/tauri-plugin-mocks.ts',
    );
    alias['@tauri-apps/api/event'] = path.resolve(
      __dirname_local,
      '../src/lib/tauri-plugin-mocks.ts',
    );
    alias['@tauri-apps/plugin-dialog'] = path.resolve(
      __dirname_local,
      '../src/lib/tauri-plugin-mocks.ts',
    );
    alias['@tauri-apps/plugin-updater'] = path.resolve(
      __dirname_local,
      '../src/lib/tauri-plugin-mocks.ts',
    );
    return rsbuildConfig;
  },
};
export default config;
