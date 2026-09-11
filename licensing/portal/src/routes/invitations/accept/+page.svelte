<script lang="ts">
  import { replaceState } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import {
    createPortalApi,
    PortalAuthenticationRequiredError,
    portalErrorMessage,
    type OrganizationInvitationAcceptance
  } from '$lib/api';
  import SignInPanel from '$lib/components/SignInPanel.svelte';

  const api = createPortalApi();
  let loading = $state(true);
  let authenticationRequired = $state(false);
  let acceptance = $state<OrganizationInvitationAcceptance | null>(null);
  let error = $state<string | null>(null);

  onMount(() => void acceptInvitation());

  async function acceptInvitation() {
    const secret = page.url.searchParams.get('secret')?.trim();
    if (!secret) {
      error = 'This invitation link is missing its acceptance secret.';
      loading = false;
      return;
    }

    loading = true;
    authenticationRequired = false;
    acceptance = null;
    error = null;
    try {
      acceptance = await api.acceptInvitation(secret);
      removeSecretFromAddress();
    } catch (reason) {
      authenticationRequired = reason instanceof PortalAuthenticationRequiredError;
      error = portalErrorMessage(reason);
      if (!authenticationRequired) removeSecretFromAddress();
    } finally {
      loading = false;
    }
  }

  function removeSecretFromAddress() {
    const url = new URL(page.url);
    url.searchParams.delete('secret');
    replaceState(`${url.pathname}${url.search}${url.hash}`, page.state);
  }

  function titleCase(value: string) {
    return value.charAt(0).toUpperCase() + value.slice(1);
  }
</script>

<svelte:head>
  <title>Accept invitation · Styrhous</title>
</svelte:head>

<section class="page-heading" aria-labelledby="invitation-heading">
  <h1 id="invitation-heading">Organization invitation</h1>
  <p class="lede">Confirm access to the organization that invited you.</p>
</section>

{#if loading}
  <section class="state-panel" aria-busy="true" aria-labelledby="accepting-heading">
    <span class="state-mark" aria-hidden="true">•••</span>
    <div>
      <p class="card-kicker">Invitation</p>
      <h2 id="accepting-heading">Accepting invitation</h2>
      <p>Confirming your organization membership and product access.</p>
    </div>
  </section>
{:else if authenticationRequired}
  <SignInPanel
    title="Sign in to accept this invitation"
    message={error ?? 'Sign in with the email address that received the invitation.'}
    onretry={() => void acceptInvitation()}
  />
{:else if acceptance}
  <section
    class="state-panel success-panel"
    role="status"
    aria-live="polite"
    aria-labelledby="invitation-accepted-heading"
  >
    <span class="state-mark" aria-hidden="true">✓</span>
    <div>
      <p class="card-kicker">Invitation accepted</p>
      <h2 id="invitation-accepted-heading">Your organization access is ready</h2>
      <p>
        You joined as {titleCase(acceptance.role)}.
        {acceptance.productSeatAssigned
          ? 'A license was assigned to you.'
          : 'No license was assigned.'}
      </p>
      <a class="primary-button action-link" href="/organizations">Open organizations</a>
    </div>
  </section>
{:else}
  <section class="state-panel error-panel" aria-labelledby="invitation-error-heading">
    <span class="state-mark" aria-hidden="true">!</span>
    <div>
      <p class="card-kicker">Invitation unavailable</p>
      <h2 id="invitation-error-heading">This invitation could not be accepted</h2>
      <p role="alert">{error ?? 'The invitation is no longer available.'}</p>
      <a class="secondary-action action-link" href="/organizations">Open organizations</a>
    </div>
  </section>
{/if}
