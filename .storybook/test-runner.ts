import type { TestRunnerConfig } from '@storybook/test-runner';
import { toMatchImageSnapshot } from 'jest-image-snapshot';
import type { Page } from 'playwright';

const SNAPSHOT_VIEWPORT = { width: 1280, height: 720 } as const;

const config: TestRunnerConfig = {
  setup() {
    expect.extend({ toMatchImageSnapshot });
  },

  async preVisit(page: Page) {
    await page.setViewportSize(SNAPSHOT_VIEWPORT);
  },

  async postVisit(page, context) {
    const root = page.locator('#storybook-root');
    const box = await root.boundingBox();
    if (!box || box.height === 0) return;

    await page.evaluate(() => document.fonts.ready);

    const image = await root.screenshot({
      animations: 'disabled',
      caret: 'hide',
    });
    expect(image).toMatchImageSnapshot({
      customSnapshotIdentifier: context.id,
      failureThreshold: 0.05,
      failureThresholdType: 'percent',
      allowSizeMismatch: true,
    });
  },
};

export default config;
