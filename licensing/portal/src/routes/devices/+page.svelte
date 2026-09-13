<script lang="ts">
  import { onMount, tick } from 'svelte';
  import DeviceRow from '$lib/components/DeviceRow.svelte';
  import SignInPanel from '$lib/components/SignInPanel.svelte';
  import {
    createPortalApi,
    PortalAuthenticationRequiredError,
    portalErrorMessage,
    type ActiveDevice,
    type DeviceList,
    type OrganizationSummary,
    type SeatEntitlement
  } from '$lib/api';
  import { formatUtcDateTime } from '$lib/date';

  type SeatWorkspace = Readonly<{
    entitlement: SeatEntitlement;
    organization: OrganizationSummary | null;
    devices: DeviceList;
  }>;

  const api = createPortalApi();

  let seats = $state<readonly SeatWorkspace[]>([]);
  let loading = $state(true);
  let authenticationRequired = $state(false);
  let pageError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let actionStatus = $state<string | null>(null);
  let busyActivationId = $state<string | null>(null);
  let actionStatusElement = $state<HTMLParagraphElement | null>(null);
  let seatLoad = 0;

  onMount(() => {
    void loadSeats();
  });

  async function loadSeats() {
    const load = ++seatLoad;
    loading = true;
    authenticationRequired = false;
    pageError = null;
    try {
      const [organizations, entitlements] = await Promise.all([
        api.listOrganizations(),
        api.listEntitlements()
      ]);
      const organizationsBySeat = new Map(
        organizations.map((organization) => [organization.seatId, organization])
      );
      const loadedSeats = await Promise.all(
        entitlements.map(async (entitlement) => ({
          entitlement,
          organization: organizationsBySeat.get(entitlement.seatId) ?? null,
          devices: await api.listDevices(entitlement.seatId)
        }))
      );
      if (load !== seatLoad) return;

      seats = loadedSeats;
      authenticationRequired = false;
    } catch (error) {
      if (load !== seatLoad) return;
      authenticationRequired = error instanceof PortalAuthenticationRequiredError;
      pageError = portalErrorMessage(error);
      seats = [];
    } finally {
      if (load === seatLoad) loading = false;
    }
  }

  async function revokeDevice(seat: SeatWorkspace, device: ActiveDevice) {
    if (!window.confirm(`Remove ${device.displayName}? It will need to be activated again for licensed access.`)) return;

    busyActivationId = device.activationId;
    actionError = null;
    actionStatus = null;
    try {
      await api.revokeDevice(device.activationId);
      seats = seats.map((candidate) =>
        candidate.entitlement.seatId === seat.entitlement.seatId
          ? {
              ...candidate,
              devices: {
                ...candidate.devices,
                activeDevices: candidate.devices.activeDevices.filter(
                  (activeDevice) => activeDevice.activationId !== device.activationId
                )
              }
            }
          : candidate
      );
      actionStatus = `${device.displayName} was removed.`;
      await tick();
      actionStatusElement?.focus();
    } catch (error) {
      const message = portalErrorMessage(error);
      if (error instanceof PortalAuthenticationRequiredError) {
        authenticationRequired = true;
        pageError = message;
        actionError = null;
      } else {
        actionError = message;
      }
    } finally {
      busyActivationId = null;
    }
  }

  function seatName(seat: SeatWorkspace) {
    return seat.organization?.name ?? 'Personal seat';
  }

  function formatReason(reasonCode: string) {
    const reasons: Readonly<Record<string, string>> = {
      active_trial: 'Trial access is active',
      trial_not_started: 'Trial access has not started',
      trial_expired: 'Trial access has ended',
      active_subscription: 'Subscription is active',
      subscription_past_due: 'Payment recovery period',
      subscription_cancels_at_period_end: 'Cancels at the end of this period',
      subscription_not_started: 'Subscription period has not started',
      subscription_expired: 'Subscription period has ended',
      subscription_inactive: 'Subscription is inactive',
      subscription_seat_capacity_exceeded: 'This seat is above purchased capacity',
      product_seat_not_assigned: 'No license assigned',
      no_valid_entitlement: 'No active license'
    };
    return reasons[reasonCode] ?? reasonCode.replaceAll('_', ' ');
  }

  function isProductSeatDisabled(seat: SeatWorkspace) {
    return seat.entitlement.reasonCode === 'product_seat_not_assigned';
  }

</script>

<svelte:head>
  <title>Devices · Styrhous</title>
</svelte:head>

<section class="page-heading" aria-labelledby="devices-heading">
  <h1 id="devices-heading">Devices</h1>
</section>

<div class="message-stack" aria-live="polite">
  {#if actionStatus}
    <p bind:this={actionStatusElement} class="success-message" role="status" tabindex="-1">
      {actionStatus}
    </p>
  {/if}
  {#if actionError}<p class="error-message" role="alert">{actionError}</p>{/if}
</div>

{#if authenticationRequired}
  <SignInPanel
    title="Sign in to review active devices"
    message={pageError ?? 'Sign in to continue.'}
    onretry={() => void loadSeats()}
  />
{:else if loading}
  <section class="state-panel" aria-busy="true" aria-label="Loading devices">
    <span class="state-mark" aria-hidden="true">•••</span>
    <div><h2>Loading devices</h2><p>Loading seats and active devices.</p></div>
  </section>
{:else if pageError}
  <section class="state-panel error-panel" aria-labelledby="device-error-heading">
    <span class="state-mark" aria-hidden="true">!</span>
    <div>
      <h2 id="device-error-heading">Devices could not be loaded</h2>
      <p role="alert">{pageError}</p>
      <button class="secondary-action" type="button" onclick={() => void loadSeats()}>
        Try again
      </button>
    </div>
  </section>
{:else if seats.length === 0}
  <section class="state-panel" aria-labelledby="empty-seats-heading">
    <span class="state-mark" aria-hidden="true">00</span>
    <div>
      <p class="card-kicker">No assigned seats</p>
      <h2 id="empty-seats-heading">No devices available</h2>
      <p>A device can be activated after a seat is assigned to your account.</p>
    </div>
  </section>
{:else}
  <div class="seat-list">
    {#each seats as seat (seat.entitlement.seatId)}
      <section class="workspace-panel seat-panel" aria-labelledby={`seat-${seat.entitlement.seatId}`}>
        <div class="seat-heading">
          <div>
            <h2 id={`seat-${seat.entitlement.seatId}`}>{seatName(seat)}</h2>
          </div>
          <div class="seat-status">
            <span class:eligible={seat.entitlement.isEligible} class="state-badge">
              {isProductSeatDisabled(seat) ? 'License inactive' : formatReason(seat.entitlement.reasonCode)}
            </span>
            <strong>{isProductSeatDisabled(seat) ? seat.devices.activeDevices.length : `${seat.devices.activeDevices.length} / ${seat.devices.deviceLimit}`}</strong>
            <span>{isProductSeatDisabled(seat) ? 'saved devices' : 'devices'}</span>
          </div>
        </div>

        {#if isProductSeatDisabled(seat)}
          <p class="entitlement-window">
            Assign a license to restore licensed access on these devices.
          </p>
        {:else if seat.entitlement.validUntil}
          <p class="entitlement-window">
            {seat.entitlement.isEligible ? 'Valid until' : 'Last valid until'}
            <time datetime={seat.entitlement.validUntil}>{formatUtcDateTime(seat.entitlement.validUntil)}</time>
          </p>
        {/if}

        {#if seat.devices.activeDevices.length === 0}
          <div class="inline-empty-state">
            <span aria-hidden="true">—</span>
            <p>No devices yet. Sign in to Styrhous on your computer to add one.</p>
          </div>
        {:else}
          <ul class="device-list">
            {#each seat.devices.activeDevices as device (device.activationId)}
              <DeviceRow {device}
                disabled={busyActivationId !== null} busy={busyActivationId === device.activationId}
                onrevoke={() => void revokeDevice(seat, device)} />
            {/each}
          </ul>
        {/if}
      </section>
    {/each}
  </div>
{/if}
