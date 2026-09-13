<script lang="ts">
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import SignInPanel from '$lib/components/SignInPanel.svelte';
  import {
    authenticationFailureMessage,
    authenticationProviderLabel,
    createPortalApi,
    PortalAuthenticationRequiredError,
    portalErrorMessage,
    type AuthenticationProvider,
    type AuthenticationSession
  } from '$lib/api';

  const api = createPortalApi();
  let session = $state<AuthenticationSession | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);
  let pendingAction = $state<string | null>(null);
  const authenticationFailure = $derived(
    authenticationFailureMessage(page.url.searchParams.get('authenticationError'))
  );

  onMount(() => void load());

  async function load() {
    loading = true;
    error = null;
    try {
      session = await api.getAuthenticationSession();
    } catch (reason) {
      error = portalErrorMessage(reason);
    } finally {
      loading = false;
    }
  }

  async function unlink(provider: AuthenticationProvider) {
    if (
      pendingAction ||
      !window.confirm(
        `Remove ${authenticationProviderLabel(provider)} from this Styrhous account?`
      )
    ) {
      return;
    }
    pendingAction = `unlink:${provider}`;
    error = null;
    status = null;
    try {
      await api.unlinkAuthenticationProvider(provider);
      status = `${authenticationProviderLabel(provider)} was unlinked.`;
      await load();
    } catch (reason) {
      if (reason instanceof PortalAuthenticationRequiredError) {
        session = { authenticated: false, configuredProviders: [] };
      } else {
        error = portalErrorMessage(reason);
      }
    } finally {
      pendingAction = null;
    }
  }

  async function signOut() {
    if (pendingAction) return;
    pendingAction = 'sign-out';
    error = null;
    status = null;
    try {
      await api.signOut();
      session = { authenticated: false, configuredProviders: session?.configuredProviders ?? [] };
    } catch (reason) {
      if (reason instanceof PortalAuthenticationRequiredError) {
        session = { authenticated: false, configuredProviders: [] };
      } else {
        error = portalErrorMessage(reason);
      }
    } finally {
      pendingAction = null;
    }
  }

  const unlinkedProviders = $derived(
    session?.authenticated
      ? session.configuredProviders.filter(
          (provider) => !session?.linkedProviders?.includes(provider)
        )
      : []
  );
</script>

<svelte:head><title>Account · Styrhous</title></svelte:head>

<section class="page-heading" aria-labelledby="account-heading">
  <h1 id="account-heading">Account</h1>
</section>

{#if loading}
  <section class="state-panel" aria-busy="true" aria-label="Loading account">
    <span class="state-mark" aria-hidden="true">•••</span>
    <div><h2>Loading account</h2></div>
  </section>
{:else if error && !session}
  <section class="state-panel error-panel">
    <span class="state-mark" aria-hidden="true">!</span>
    <div>
      <h2>Account could not be loaded</h2>
      <p role="alert">{error}</p>
      <button class="secondary-action" type="button" onclick={() => void load()}>Try again</button>
    </div>
  </section>
{:else if !session?.authenticated}
  <SignInPanel title="Sign in to manage your account" />
{:else}
  <div class="message-stack" aria-live="polite">
    {#if authenticationFailure}<p class="error-message" role="alert">{authenticationFailure}</p>{/if}
    {#if status}<p class="success-message" role="status">{status}</p>{/if}
    {#if error}<p class="error-message" role="alert">{error}</p>{/if}
  </div>
  <section
    class="workspace-panel account-panel"
    aria-labelledby="identity-heading"
    aria-busy={pendingAction !== null}
  >
    <div class="seat-heading">
      <div>
        <h2 id="identity-heading">{session.email}</h2>
      </div>
      <button
        class="secondary-action"
        type="button"
        disabled={pendingAction !== null}
        onclick={() => void signOut()}
      >Sign out</button>
    </div>

    <h3>Sign-in methods</h3>
    <ul class="account-provider-list">
      {#each session.linkedProviders ?? [] as provider}
        <li>
          <strong>{authenticationProviderLabel(provider)}</strong>
          {#if (session.linkedProviders?.length ?? 0) > 1 && session.recentlyAuthenticated}
            <button
              class="secondary-action danger-text"
              type="button"
              aria-label={`Remove ${authenticationProviderLabel(provider)}`}
              disabled={pendingAction !== null}
              onclick={() => void unlink(provider)}
            >
              Remove
            </button>
          {:else if (session.linkedProviders?.length ?? 0) > 1}
            <a
              class="secondary-action action-link"
              href={`/auth/reauth/${provider}?returnUrl=/account`}
            >
              Sign in again to remove
            </a>
          {:else}
            <span class="state-badge">Required</span>
          {/if}
        </li>
      {/each}
    </ul>

    <h3>Add a sign-in method</h3>
    <div class="button-row">
      {#each unlinkedProviders as provider}
        {#if session.recentlyAuthenticated}
          <a
            class="secondary-action action-link"
            href={`/auth/link/${provider}?returnUrl=/account`}
          >
            Add {authenticationProviderLabel(provider)}
          </a>
        {:else}
          <a
            class="secondary-action action-link"
            href={`/auth/reauth/${session.linkedProviders?.[0]}?returnUrl=/account`}
          >
            Sign in again to add {authenticationProviderLabel(provider)}
          </a>
        {/if}
      {/each}
      {#if session.configuredProviders.length === 0}
        <p class="muted-copy">No other sign-in methods are available.</p>
      {:else if unlinkedProviders.length === 0}
        <p class="muted-copy">All available sign-in methods are connected.</p>
      {/if}
    </div>
  </section>
{/if}
