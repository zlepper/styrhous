import { cloudflareTest, readD1Migrations } from '@cloudflare/vitest-plugin';
import { defineConfig } from 'vitest/config';
import { testBindings } from './test/test-config.ts';

export default defineConfig(async () => ({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: './wrangler.jsonc' },
      miniflare: {
        bindings: {
          ...testBindings,
          TEST_MIGRATIONS: await readD1Migrations('./migrations')
        },
        assets: { directory: './test/assets' }
      }
    })
  ],
  test: {
    setupFiles: ['./test/apply-migrations.ts'],
    // Undo FakeUpstream's fetch stub and email spies after every test.
    unstubGlobals: true,
    restoreMocks: true
  }
}));
