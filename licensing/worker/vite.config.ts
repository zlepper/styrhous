import { cloudflare } from '@cloudflare/vite-plugin';
import { defineConfig } from 'vite';

// The static portal build is published as the Worker's assets.
export default defineConfig({
  publicDir: '../portal/build',
  plugins: [cloudflare()]
});
