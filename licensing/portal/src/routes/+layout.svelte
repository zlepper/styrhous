<script lang="ts">
  import { page } from '$app/state';
  import { applicationIcon } from '$lib/branding';
  import { isCurrentPortalRoute, portalNavigation } from '$lib/navigation';
  import '../app.css';

  let { children } = $props();
</script>

<svelte:head>
  <meta
    name="description"
    content="Manage Styrhous trials, organizations, seats, devices, and billing."
  />
  <link rel="icon" href={applicationIcon} type="image/svg+xml" />
</svelte:head>

{#if page.url.pathname.startsWith('/showcase/')}
  {@render children()}
{:else}
  <a class="skip-link" href="#main-content">Skip to main content</a>

  <div class="app-shell">
    <header class="site-header">
      <div class="site-header-inner">
        <a class="brand" href="/" aria-label="Styrhous licensing home">
          <img class="brand-mark" src={applicationIcon} alt="" />
          <span>
            <strong>Styrhous</strong>
            <small>Licensing</small>
          </span>
        </a>

        <nav class="primary-navigation" aria-label="Primary navigation">
          <ul>
            {#each portalNavigation as item}
              <li>
                <a
                  href={item.href}
                  aria-current={isCurrentPortalRoute(page.url.pathname, item.href) ? 'page' : undefined}
                >
                  {item.label}
                </a>
              </li>
            {/each}
          </ul>
        </nav>
      </div>
    </header>

    <main id="main-content" tabindex="-1">
      {@render children()}
    </main>

    <footer class="site-footer">
      <span>Styrhous licensing</span>
    </footer>
  </div>
{/if}
