<script lang="ts">
  import { page } from '$app/state';
  import { tick } from 'svelte';
  import DeviceRow from '$lib/components/DeviceRow.svelte';
  import SignInPanel from '$lib/components/SignInPanel.svelte';
  import {
    createPortalApi,
    PortalAuthenticationRequiredError,
    portalErrorMessage,
    type ActiveDevice,
    type DeviceAuthorizationApproval,
    type DeviceAuthorizationSeat
  } from '$lib/api';

  const api = createPortalApi();
  const userCode = $derived(page.url.searchParams.get('user_code')?.trim() ?? '');

  let approval = $state<DeviceAuthorizationApproval | null>(null);
  let selectedSeatId = $state('');
  let loading = $state(true);
  let submitting = $state(false);
  let busyActivationId = $state<string | null>(null);
  let authenticationRequired = $state(false);
  let pageError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let completedMessage = $state<string | null>(null);
  let statusElement = $state<HTMLParagraphElement | null>(null);
  let approvalLoadId = 0;
  let authorizationGeneration = 0;

  const selectedSeat = $derived(
    approval?.eligibleSeats.find((seat) => seat.seatId === selectedSeatId) ?? null
  );

  $effect(() => {
    const requestedCode = userCode;
    const controller = new AbortController();
    authorizationGeneration += 1;
    submitting = false;
    busyActivationId = null;
    completedMessage = null;
    void loadApproval(true, requestedCode, controller.signal);
    return () => controller.abort();
  });

  async function loadApproval(
    clearActionError = true,
    requestedCode = userCode,
    signal?: AbortSignal
  ) {
    const loadId = ++approvalLoadId;
    loading = true;
    pageError = null;
    authenticationRequired = false;
    approval = null;
    if (clearActionError) actionError = null;
    if (!requestedCode) {
      loading = false;
      pageError = 'The desktop authorization link does not contain a user code.';
      return;
    }

    try {
      const loaded = await api.getDeviceAuthorizationApproval(requestedCode, signal);
      if (loadId !== approvalLoadId) return;
      approval = loaded;
      const existingSelection = loaded.eligibleSeats.some(
        (seat) => seat.seatId === selectedSeatId
      )
        ? selectedSeatId
        : null;
      selectedSeatId =
        loaded.selectedSeatId
        ?? existingSelection
        ?? (loaded.eligibleSeats.length === 1 ? loaded.eligibleSeats[0].seatId : '');
    } catch (error) {
      if (signal?.aborted || loadId !== approvalLoadId) return;
      authenticationRequired = error instanceof PortalAuthenticationRequiredError;
      pageError = portalErrorMessage(error);
      approval = null;
    } finally {
      if (loadId === approvalLoadId) loading = false;
    }
  }

  async function approve() {
    if (!selectedSeat?.canActivate || submitting || completedMessage) return;

    const actionGeneration = authorizationGeneration;
    const requestedCode = userCode;
    submitting = true;
    actionError = null;
    try {
      await api.approveDeviceAuthorization(requestedCode, selectedSeatId);
      if (!isCurrentAuthorization(actionGeneration, requestedCode)) return;
      completedMessage = `${approval?.installation.displayName ?? 'The desktop'} is approved. You can return to Styrhous.`;
      await focusStatus();
    } catch (error) {
      if (!isCurrentAuthorization(actionGeneration, requestedCode)) return;
      actionError = portalErrorMessage(error);
      await loadApproval(false, requestedCode);
      if (!isCurrentAuthorization(actionGeneration, requestedCode)) return;
      await focusStatus();
    } finally {
      if (isCurrentAuthorization(actionGeneration, requestedCode)) submitting = false;
    }
  }

  async function deny() {
    if (submitting || completedMessage) return;

    const actionGeneration = authorizationGeneration;
    const requestedCode = userCode;
    submitting = true;
    actionError = null;
    try {
      await api.denyDeviceAuthorization(requestedCode);
      if (!isCurrentAuthorization(actionGeneration, requestedCode)) return;
      completedMessage = 'The desktop authorization was denied. You can close this page.';
      await focusStatus();
    } catch (error) {
      if (!isCurrentAuthorization(actionGeneration, requestedCode)) return;
      actionError = portalErrorMessage(error);
      await focusStatus();
    } finally {
      if (isCurrentAuthorization(actionGeneration, requestedCode)) submitting = false;
    }
  }

  async function revoke(device: ActiveDevice) {
    if (busyActivationId || submitting) return;
    if (!window.confirm(`Remove ${device.displayName}? It will need to be activated again for licensed access.`)) return;

    const actionGeneration = authorizationGeneration;
    const requestedCode = userCode;
    busyActivationId = device.activationId;
    actionError = null;
    try {
      await api.revokeDevice(device.activationId);
      if (!isCurrentAuthorization(actionGeneration, requestedCode)) return;
      await loadApproval(false, requestedCode);
    } catch (error) {
      if (!isCurrentAuthorization(actionGeneration, requestedCode)) return;
      actionError = portalErrorMessage(error);
      await focusStatus();
    } finally {
      if (isCurrentAuthorization(actionGeneration, requestedCode)) busyActivationId = null;
    }
  }

  function isCurrentAuthorization(generation: number, requestedCode: string) {
    return generation === authorizationGeneration && requestedCode === userCode;
  }

  async function focusStatus() {
    await tick();
    statusElement?.focus();
  }

  function capacityLabel(seat: DeviceAuthorizationSeat) {
    return `${seat.activeDevices.length} of ${seat.deviceLimit} devices active`;
  }
</script>

<svelte:head>
  <title>Authorize device · Styrhous</title>
</svelte:head>

<section class="page-heading" aria-labelledby="authorization-heading">
  <h1 id="authorization-heading">Authorize device</h1>
  <p class="lede">Only approve a sign-in you started in Styrhous.</p>
</section>

<div class="message-stack" aria-live="polite">
  {#if completedMessage}
    <p bind:this={statusElement} class="success-message" role="status" tabindex="-1">
      {completedMessage}
    </p>
  {:else if actionError}
    <p bind:this={statusElement} class="error-message" role="alert" tabindex="-1">
      {actionError}
    </p>
  {/if}
</div>

{#if authenticationRequired}
  <SignInPanel
    title="Sign in before approving this desktop"
    message={pageError ?? 'Sign in to continue.'}
    onretry={() => void loadApproval()}
  />
{:else if loading}
  <section class="state-panel" aria-busy="true" aria-label="Loading desktop authorization">
    <span class="state-mark" aria-hidden="true">•••</span>
    <div>
      <h2>Checking the desktop request</h2>
      <p>Loading device details and available seats.</p>
    </div>
  </section>
{:else if pageError || !approval}
  <section class="state-panel error-panel" aria-labelledby="authorization-error-heading">
    <span class="state-mark" aria-hidden="true">!</span>
    <div>
      <h2 id="authorization-error-heading">This desktop request cannot be approved</h2>
      <p role="alert">{pageError ?? 'The authorization request is unavailable.'}</p>
    </div>
  </section>
{:else}
  <section class="workspace-panel authorization-panel" aria-labelledby="installation-heading">
    <div class="seat-heading">
      <div>
        <h2 id="installation-heading">{approval.installation.displayName}</h2>
        <p>{approval.installation.platform} · Styrhous {approval.installation.styrhousVersion}</p>
      </div>
      {#if !completedMessage}<span class="state-badge">Waiting for approval</span>{/if}
    </div>

    {#if !completedMessage}
      <p class="verification-code">Check this code matches Styrhous: <strong><code>{userCode}</code></strong></p>
    {/if}

    {#if completedMessage}
      <a class="secondary-action action-link" href="/devices">View devices</a>
    {:else if approval.eligibleSeats.length === 0}
      <div class="inline-empty-state">
        <span aria-hidden="true">—</span>
        <p>No license is available. Check Billing or ask your organization to assign one.</p>
      </div>
    {:else}
      {#if approval.eligibleSeats.length === 1 && selectedSeat}
        <p class="selected-license"><strong>{selectedSeat.name}</strong> · {capacityLabel(selectedSeat)}</p>
      {:else}
      <fieldset class="authorization-seats" aria-describedby="seat-selection-help">
        <legend>Choose a license</legend>
        <p id="seat-selection-help">
          Choose which license to use on this device.
        </p>
        <div class="authorization-seat-grid">
          {#each approval.eligibleSeats as seat (seat.seatId)}
            <label class:selected={selectedSeatId === seat.seatId} class="authorization-seat-option">
              <input
                type="radio"
                name="authorization-seat"
                value={seat.seatId}
                bind:group={selectedSeatId}
                disabled={submitting}
              />
              <span>
                <strong>{seat.name}</strong>
                <small>{capacityLabel(seat)}</small>
              </span>
            </label>
          {/each}
        </div>
      </fieldset>
      {/if}

      {#if selectedSeat && !selectedSeat.canActivate}
        <section class="authorization-capacity" aria-labelledby="capacity-heading">
          <div>
            <h3 id="capacity-heading">Make room before approving</h3>
            <p id="capacity-help">
              Remove a device below or choose another license. Devices inactive for seven days can be replaced automatically.
            </p>
          </div>
          <ul class="device-list">
            {#each selectedSeat.activeDevices as device (device.activationId)}
              <DeviceRow {device} disabled={busyActivationId !== null || submitting}
                busy={busyActivationId === device.activationId} onrevoke={() => void revoke(device)} />
            {/each}
          </ul>
        </section>
      {/if}

      <div class="record-actions authorization-actions">
        <button
          class="primary-button"
          type="button"
          disabled={!selectedSeat?.canActivate || submitting}
          aria-describedby={!selectedSeatId
            ? 'seat-selection-help'
            : selectedSeat && !selectedSeat.canActivate
              ? 'capacity-help'
              : undefined}
          onclick={() => void approve()}
        >
          {submitting ? 'Submitting…' : 'Approve sign-in'}
        </button>
        <button
          class="secondary-action"
          type="button"
          disabled={submitting}
          onclick={() => void deny()}
        >
          Deny sign-in
        </button>
      </div>
    {/if}
  </section>
{/if}
