<script lang="ts">
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import {
    authenticationFailureMessage,
    authenticationProviderLabel,
    createPortalApi,
    portalErrorMessage,
    type AuthenticationProvider
  } from '$lib/api';

  let {
    title = 'Sign in to Styrhous',
    message = '',
    onretry
  }: { title?: string; message?: string; onretry?: () => void } = $props();

  const api = createPortalApi();
  let providers = $state<readonly AuthenticationProvider[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  const authenticationFailure = $derived(
    authenticationFailureMessage(page.url.searchParams.get('authenticationError'))
  );
  const returnUrl = $derived.by(() => {
    const search = new URLSearchParams(page.url.searchParams);
    search.delete('authenticationError');
    const query = search.toString();
    return `${page.url.pathname}${query ? `?${query}` : ''}`;
  });

  onMount(() => {
    void loadProviders();
    const refreshSession = () => onretry?.();
    window.addEventListener('focus', refreshSession);
    return () => window.removeEventListener('focus', refreshSession);
  });

  async function loadProviders() {
    loading = true;
    error = null;
    try {
      providers = await api.listAuthenticationProviders();
    } catch (reason) {
      error = portalErrorMessage(reason);
    } finally {
      loading = false;
    }
  }
</script>

<section class="state-panel" aria-labelledby="sign-in-panel-heading">
  <span class="state-mark" aria-hidden="true">↗</span>
  <div>
    <h2 id="sign-in-panel-heading">{title}</h2>
    {#if message}<p>{message}</p>{/if}
    {#if authenticationFailure}
      <p class="error-message" role="alert">{authenticationFailure}</p>
    {/if}
    {#if loading}
      <p role="status">Loading sign-in options…</p>
    {:else if error}
      <p class="error-message" role="alert">{error}</p>
      <button class="secondary-action" type="button" onclick={() => void loadProviders()}>
        Retry sign-in options
      </button>
    {:else if providers.length === 0}
      <p role="alert">Sign-in is unavailable. Contact your administrator.</p>
    {:else}
      <div class="button-row" aria-label="Sign-in providers">
        {#each providers as provider}
          <a
            class="primary-button action-link"
            href={`/auth/sign-in/${provider}?returnUrl=${encodeURIComponent(returnUrl)}`}
          >Sign in with {authenticationProviderLabel(provider)}</a>
        {/each}
      </div>
    {/if}
  </div>
</section>
