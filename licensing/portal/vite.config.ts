import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    sveltekit({
      csp: {
        mode: 'hash',
        directives: {
          'default-src': ['self'],
          'base-uri': ['self'],
          'connect-src': ['self'],
          'font-src': ['self'],
          'img-src': ['self', 'data:'],
          'object-src': ['none'],
          'script-src': ['self'],
          'style-src': ['self', 'unsafe-inline']
        }
      },
      adapter: adapter({
        fallback: 'index.html'
      })
    })
  ]
});
