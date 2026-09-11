<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import SignInPanel from '$lib/components/SignInPanel.svelte';
  import {
    billingCheckoutErrorDetails,
    billingSeatQuantityErrorDetails,
    createPortalApi,
    PortalApiError,
    PortalAuthenticationRequiredError,
    portalErrorMessage,
    type BillingAccount,
    type BillingCadence,
    type BillingPrice
  } from '$lib/api';
  import SelectControl from '$lib/components/SelectControl.svelte';
  import { formatUtcDateTime } from '$lib/date';

  const api = createPortalApi();

  let accounts = $state<readonly BillingAccount[]>([]);
  let prices = $state<readonly BillingPrice[]>([]);
  let loadingPrices = $state(true);
  let loading = $state(true);
  let authenticationRequired = $state(false);
  let pageError = $state<string | null>(null);
  let accountLoad = 0;
  let checkoutDrafts = $state<Record<string, CheckoutDraft>>({});
  let seatQuantityDrafts = $state<Record<string, SeatQuantityDraft>>({});
  let customerPortalStates = $state<Record<string, CustomerPortalState>>({});
  let checkoutReturnMessage = $state<CheckoutReturnMessage | null>(null);
  const checkoutBlockTimers = new Set<ReturnType<typeof setTimeout>>();
  const billingRequestControllers = new Set<AbortController>();
  let pageActive = true;

  type CheckoutDraft = {
    cadence: BillingCadence;
    seatQuantity: number;
    billingOperationId: string | null;
    blockedUntil: string | null;
    pending: boolean;
    error: string | null;
  };

  type CheckoutReturnMessage = {
    message: string;
    tone: 'success' | 'notice';
  };

  type CustomerPortalState = {
    pending: boolean;
    error: string | null;
  };

  type SeatQuantityDraft = {
    seatQuantity: number;
    billingOperationId: string | null;
    pending: boolean;
    error: string | null;
    success: string | null;
  };

  onMount(() => {
    checkoutReturnMessage = checkoutReturnMessageFrom(window.location.search);
    void loadAccounts();
  });

  async function loadPrices() {
    loadingPrices = true;
    try {
      const loaded = await api.listBillingPrices();
      if (pageActive) prices = loaded;
    } catch {
      // Price discovery must not prevent account management or checkout.
    } finally {
      if (pageActive) loadingPrices = false;
    }
  }

  function priceLabel(draft: CheckoutDraft) {
    const price = prices.find((candidate) => candidate.cadence === draft.cadence);
    if (!price) return loadingPrices ? 'Loading price…' : 'Price available at checkout.';
    const formatter = new Intl.NumberFormat('en', { style: 'currency', currency: price.currency, currencyDisplay: 'code' });
    const period = draft.cadence === 'monthly' ? 'month' : 'year';
    return `${formatter.format(price.unitAmount * draft.seatQuantity)} / ${period} · ${formatter.format(price.unitAmount)} per seat · ${price.taxIncluded === null ? 'tax confirmed at checkout' : price.taxIncluded ? 'tax included' : 'before tax'}`;
  }

  onDestroy(() => {
    pageActive = false;
    for (const timer of checkoutBlockTimers) clearTimeout(timer);
    for (const controller of billingRequestControllers) controller.abort();
    billingRequestControllers.clear();
  });

  async function loadAccounts(): Promise<readonly BillingAccount[] | null> {
    const load = ++accountLoad;
    loading = true;
    authenticationRequired = false;
    pageError = null;
    try {
      const loadedAccounts = await api.listBillingAccounts();
      if (load !== accountLoad) return null;

      accounts = loadedAccounts;
      if (prices.length === 0) void loadPrices();
      checkoutDrafts = Object.fromEntries(
        loadedAccounts
          .filter((account) => account.canStartCheckout)
          .map((account) => [account.billingAccountId, createCheckoutDraft(account)])
      );
      const existingSeatQuantityDrafts = seatQuantityDrafts;
      seatQuantityDrafts = Object.fromEntries(
        loadedAccounts
          .filter((account) =>
            account.canManageBilling
            && account.accountKind === 'organization'
            && account.subscription !== null
          )
          .map((account) => [
            account.billingAccountId,
            refreshSeatQuantityDraft(
              account,
              existingSeatQuantityDrafts[account.billingAccountId]
            )
          ])
      );
      customerPortalStates = Object.fromEntries(
        loadedAccounts
          .filter((account) => account.canManageBilling && account.subscription !== null)
          .map((account) => [account.billingAccountId, { pending: false, error: null }])
      );
      return loadedAccounts;
    } catch (error) {
      if (load !== accountLoad) return null;
      authenticationRequired = error instanceof PortalAuthenticationRequiredError;
      pageError = portalErrorMessage(error);
      clearBillingState();
      return null;
    } finally {
      if (load === accountLoad) loading = false;
    }
  }

  function accountName(account: BillingAccount) {
    return account.organizationName ?? 'Personal plan';
  }


  function needsPayment(account: BillingAccount) {
    return ['past_due', 'unpaid', 'incomplete'].includes(account.subscription?.status ?? '');
  }

  function statusLabel(account: BillingAccount) {
    const labels: Readonly<Record<string, string>> = {
      active_trial: 'Active trial',
      trial_not_started: 'Trial not started',
      trial_expired: 'Trial ended',
      active_subscription: 'Active subscription',
      subscription_past_due: 'Payment overdue',
      subscription_cancels_at_period_end: 'Cancels at period end',
      subscription_not_started: 'Subscription not started',
      subscription_expired: 'Subscription ended',
      subscription_inactive: 'Subscription inactive',
      subscription_seat_capacity_exceeded: 'Seat above purchased capacity',
      product_seat_not_assigned: 'No license assigned',
      no_valid_entitlement: 'No active plan'
    };
    return labels[account.entitlement.reasonCode]
      ?? account.entitlement.reasonCode.replaceAll('_', ' ');
  }

  function isCurrent(account: BillingAccount) {
    return account.entitlement.isEligible && account.entitlement.reasonCode !== 'subscription_past_due';
  }

  function entitlementWindow(account: BillingAccount) {
    const entitlement = account.entitlement;
    if (entitlement.isEligible && entitlement.validUntil) {
      return { label: 'Access until', value: entitlement.validUntil };
    }

    if (entitlement.reasonCode.endsWith('_not_started') && entitlement.validFrom) {
      return { label: 'Period starts', value: entitlement.validFrom };
    }

    if (entitlement.reasonCode.endsWith('_expired') && entitlement.validUntil) {
      return { label: 'Period ended', value: entitlement.validUntil };
    }

    if (entitlement.validUntil) {
      return { label: 'Billing period ends', value: entitlement.validUntil };
    }

    return null;
  }

  function createCheckoutDraft(account: BillingAccount): CheckoutDraft {
    return {
      cadence: 'monthly',
      seatQuantity: account.accountKind === 'personal' ? 1 : Math.max(1, account.assignedSeatCount),
      billingOperationId: null,
      blockedUntil: null,
      pending: false,
      error: null
    };
  }

  function createSeatQuantityDraft(account: BillingAccount): SeatQuantityDraft {
    return {
      seatQuantity: account.subscription?.seatQuantity ?? 1,
      billingOperationId: null,
      pending: false,
      error: null,
      success: null
    };
  }

  function refreshSeatQuantityDraft(
    account: BillingAccount,
    existing: SeatQuantityDraft | undefined
  ): SeatQuantityDraft {
    if (!existing) return createSeatQuantityDraft(account);

    existing.seatQuantity = account.subscription?.seatQuantity ?? 1;
    existing.billingOperationId = null;
    existing.error = null;
    existing.success = null;
    return existing;
  }

  function updateCadence(draft: CheckoutDraft, cadence: BillingCadence) {
    draft.cadence = cadence;
    resetCheckoutAttempt(draft);
  }

  function updateSeatQuantity(draft: CheckoutDraft, value: string) {
    draft.seatQuantity = Number(value);
    resetCheckoutAttempt(draft);
  }

  function resetCheckoutAttempt(draft: CheckoutDraft) {
    draft.billingOperationId = null;
    draft.blockedUntil = null;
    draft.error = null;
  }

  function updateSubscriptionSeatQuantity(draft: SeatQuantityDraft, value: string) {
    draft.seatQuantity = Number(value);
    draft.billingOperationId = null;
    draft.error = null;
    draft.success = null;
  }

  async function changeSubscriptionSeatQuantity(
    account: BillingAccount,
    draft: SeatQuantityDraft
  ) {
    if (draft.pending) return;

    const controller = new AbortController();
    billingRequestControllers.add(controller);
    draft.pending = true;
    draft.error = null;
    draft.success = null;
    try {
      const result = await api.changeSeatQuantity(
        account.billingAccountId,
        draft.seatQuantity,
        draft.billingOperationId,
        controller.signal
      );
      if (!pageActive || controller.signal.aborted) return;
      const refreshedAccounts = await loadAccounts();
      if (!pageActive || !refreshedAccounts) return;

      const authoritativeQuantity = refreshedAccounts.find(
        (candidate) => candidate.billingAccountId === account.billingAccountId
      )?.subscription?.seatQuantity;
      if (authoritativeQuantity !== undefined) {
        const refreshedDraft = seatQuantityDrafts[account.billingAccountId];
        draft.seatQuantity = authoritativeQuantity;
        draft.success = authoritativeQuantity === result.seatQuantity
          ? `Purchased seats changed to ${result.seatQuantity}.`
          : null;
        if (refreshedDraft && refreshedDraft !== draft) {
          refreshedDraft.seatQuantity = draft.seatQuantity;
          refreshedDraft.success = draft.success;
        }
      }
    } catch (error) {
      if (controller.signal.aborted) return;
      if (error instanceof PortalAuthenticationRequiredError) {
        showAuthenticationRequired(error);
        return;
      }

      if (error instanceof PortalApiError) {
        const details = billingSeatQuantityErrorDetails(error);
        if (details.requiredSeatQuantity
          && details.requiredSeatQuantity > draft.seatQuantity) {
          draft.seatQuantity = details.requiredSeatQuantity;
        }
        if (error.reasonCode === 'seat_quantity_operation_in_progress'
          && details.billingOperationId
          && details.seatQuantity) {
          draft.billingOperationId = details.billingOperationId;
          draft.seatQuantity = details.seatQuantity;
        } else if (error.reasonCode === 'seat_quantity_provider_unavailable'
          || error.reasonCode === 'seat_quantity_reconciliation_required') {
          draft.billingOperationId = details.billingOperationId;
        } else {
          draft.billingOperationId = null;
        }

        if (error.reasonCode === 'subscription_quantity_changed'
          && details.seatQuantity) {
          const refreshedAccounts = await loadAccounts();
          if (!pageActive || !refreshedAccounts) return;
          const authoritativeQuantity = refreshedAccounts.find(
            (candidate) => candidate.billingAccountId === account.billingAccountId
          )?.subscription?.seatQuantity;
          const refreshedDraft = seatQuantityDrafts[account.billingAccountId];
          if (refreshedDraft) {
            if (authoritativeQuantity !== undefined) {
              refreshedDraft.seatQuantity = authoritativeQuantity;
            }
            refreshedDraft.error = portalErrorMessage(error);
          }
          await focusSeatQuantityError(account);
          return;
        }
      }

      draft.error = portalErrorMessage(error);
      await focusSeatQuantityError(account);
    } finally {
      billingRequestControllers.delete(controller);
      if (pageActive) draft.pending = false;
    }
  }

  async function focusSeatQuantityError(account: BillingAccount) {
    await tick();
    document.getElementById(seatQuantityErrorId(account))?.focus();
  }

  async function startCheckout(account: BillingAccount, draft: CheckoutDraft) {
    if (draft.blockedUntil && Date.parse(draft.blockedUntil) <= Date.now()) {
      draft.blockedUntil = null;
      draft.error = null;
    }
    if (draft.pending || draft.blockedUntil) return;

    draft.pending = true;
    draft.error = null;
    try {
      const session = await api.createCheckoutSession(
        account.billingAccountId,
        draft.cadence,
        draft.seatQuantity,
        draft.billingOperationId
      );
      window.location.assign(session.redirectUrl);
    } catch (error) {
      if (error instanceof PortalAuthenticationRequiredError) {
        showAuthenticationRequired(error);
        return;
      }

      if (error instanceof PortalApiError) {
        const details = billingCheckoutErrorDetails(error);
        if (details.requiredSeatQuantity && details.requiredSeatQuantity > draft.seatQuantity) {
          draft.seatQuantity = details.requiredSeatQuantity;
        }
        if (
          error.reasonCode === 'checkout_operation_in_progress'
          && details.billingOperationId
          && details.cadence
          && details.seatQuantity
        ) {
          draft.cadence = details.cadence;
          draft.seatQuantity = details.seatQuantity;
          draft.billingOperationId = details.billingOperationId;
        } else if (error.reasonCode === 'checkout_operation_capacity_changed') {
          draft.billingOperationId = null;
          blockCheckoutUntil(draft, details.expiresAt);
        } else {
          draft.billingOperationId = error.reasonCode === 'checkout_provider_unavailable'
            ? details.billingOperationId
            : null;
        }
      }
      draft.error = error instanceof PortalApiError
        && error.reasonCode === 'checkout_operation_capacity_changed'
        && billingCheckoutErrorDetails(error).expiresAt
          ? `This checkout has too few seats for your members and invitations. Start a new checkout after ${formatUtcDateTime(billingCheckoutErrorDetails(error).expiresAt!)}.`
          : portalErrorMessage(error);
      await tick();
      document.getElementById(checkoutErrorId(account))?.focus();
    } finally {
      draft.pending = false;
    }
  }

  function blockCheckoutUntil(draft: CheckoutDraft, expiresAt: string | null) {
    draft.blockedUntil = expiresAt;
    if (!expiresAt) return;

    const remainingMilliseconds = Date.parse(expiresAt) - Date.now();
    if (remainingMilliseconds <= 0) {
      draft.blockedUntil = null;
      return;
    }

    const timer = setTimeout(() => {
      checkoutBlockTimers.delete(timer);
      if (draft.blockedUntil === expiresAt) {
        draft.blockedUntil = null;
        draft.error = null;
      }
    }, remainingMilliseconds);
    checkoutBlockTimers.add(timer);
  }

  function checkoutErrorId(account: BillingAccount) {
    return `checkout-error-${account.billingAccountId}`;
  }

  async function openCustomerPortal(account: BillingAccount, state: CustomerPortalState) {
    if (state.pending) return;

    const controller = new AbortController();
    billingRequestControllers.add(controller);
    state.pending = true;
    state.error = null;
    try {
      const session = await api.createCustomerPortalSession(
        account.billingAccountId,
        controller.signal
      );
      if (!pageActive || controller.signal.aborted) return;
      window.location.assign(session.redirectUrl);
    } catch (error) {
      if (controller.signal.aborted) return;
      if (error instanceof PortalAuthenticationRequiredError) {
        showAuthenticationRequired(error);
        return;
      }

      state.error = portalErrorMessage(error);
      await tick();
      document.getElementById(customerPortalErrorId(account))?.focus();
    } finally {
      billingRequestControllers.delete(controller);
      if (pageActive) state.pending = false;
    }
  }

  function showAuthenticationRequired(error: PortalAuthenticationRequiredError) {
    authenticationRequired = true;
    pageError = portalErrorMessage(error);
    clearBillingState();
  }

  function clearBillingState() {
    accounts = [];
    checkoutDrafts = {};
    seatQuantityDrafts = {};
    customerPortalStates = {};
  }

  function customerPortalErrorId(account: BillingAccount) {
    return `customer-portal-error-${account.billingAccountId}`;
  }

  function seatQuantityErrorId(account: BillingAccount) {
    return `seat-quantity-error-${account.billingAccountId}`;
  }

  function seatQuantitySuccessId(account: BillingAccount) {
    return `seat-quantity-success-${account.billingAccountId}`;
  }

  function checkoutReturnMessageFrom(search: string) {
    const result = new URLSearchParams(search).get('checkout');
    if (result === 'success') {
      return {
        message: 'Checkout completed. Your billing status will update after payment is confirmed.',
        tone: 'success' as const
      };
    }

    if (result === 'cancelled') {
      return {
        message: 'Checkout was cancelled. No subscription changes were made.',
        tone: 'notice' as const
      };
    }

    return null;
  }
</script>

<svelte:head>
  <title>Billing · Styrhous</title>
</svelte:head>

<section class="page-heading" aria-labelledby="billing-heading">
  <h1 id="billing-heading">Billing</h1>
</section>

{#if checkoutReturnMessage}
  <p
    class:success-message={checkoutReturnMessage.tone === 'success'}
    class:notice-message={checkoutReturnMessage.tone === 'notice'}
    class="checkout-return"
    role="status"
  >{checkoutReturnMessage.message}</p>
{/if}

{#if authenticationRequired}
  <SignInPanel
    title="Sign in to review billing"
    message={pageError ?? 'Sign in to continue.'}
    onretry={() => void loadAccounts()}
  />
{:else if loading}
  <section class="state-panel" aria-busy="true" aria-label="Loading billing accounts">
    <span class="state-mark" aria-hidden="true">•••</span>
    <div>
      <h2>Loading billing accounts</h2>
      <p>Loading trials, subscriptions, and seat assignments.</p>
    </div>
  </section>
{:else if pageError}
  <section class="state-panel error-panel" aria-labelledby="billing-error-heading">
    <span class="state-mark" aria-hidden="true">!</span>
    <div>
      <h2 id="billing-error-heading">Billing could not be loaded</h2>
      <p role="alert">{pageError}</p>
      <button class="secondary-action" type="button" onclick={() => void loadAccounts()}>
        Try again
      </button>
    </div>
  </section>
{:else if accounts.length === 0}
  <section class="state-panel" aria-labelledby="billing-empty-heading">
    <span class="state-mark" aria-hidden="true">00</span>
    <div>
      <p class="card-kicker">No billing accounts</p>
      <h2 id="billing-empty-heading">No billing accounts found</h2>
      <p>Your personal billing account is created during signup.</p>
    </div>
  </section>
{:else}
  <div class="billing-grid">
    {#each accounts as account (account.billingAccountId)}
      {@const window = entitlementWindow(account)}
      {@const draft = checkoutDrafts[account.billingAccountId]}
      {@const seatQuantityDraft = seatQuantityDrafts[account.billingAccountId]}
      {@const customerPortalState = customerPortalStates[account.billingAccountId]}
      <article class="workspace-panel billing-card" aria-labelledby={`billing-${account.billingAccountId}`}>
        <div class="billing-card-heading">
          <div>
            <h2 id={`billing-${account.billingAccountId}`}>{accountName(account)}</h2>
          </div>
          <div class="billing-status">
            <span class:eligible={isCurrent(account)} class:warning={account.entitlement.reasonCode === 'subscription_past_due'} class="state-badge">{statusLabel(account)}</span>
            {#if needsPayment(account) && account.entitlement.reasonCode !== 'subscription_past_due'}
              <span class="state-badge warning">{account.subscription?.status === 'incomplete' ? 'Payment incomplete' : 'Payment overdue'}</span>
            {/if}
          </div>
        </div>

        <dl class="billing-facts">
          <div>
            <dt>Seats in use</dt>
            <dd>{account.subscription ? `${account.assignedSeatCount} / ${account.subscription.seatQuantity}` : account.assignedSeatCount}</dd>
          </div>
          {#if window}
            <div>
              <dt>{window.label}</dt>
              <dd>
                <time datetime={window.value}>
                  {formatUtcDateTime(window.value)}
                </time>
              </dd>
            </div>
          {/if}
        </dl>

        {#if account.canStartCheckout && draft}
          <form
            class="checkout-form"
            aria-label={`Purchase ${accountName(account)}`}
            aria-busy={draft.pending}
            aria-describedby={draft.error ? checkoutErrorId(account) : undefined}
            onsubmit={(event) => {
              event.preventDefault();
              void startCheckout(account, draft);
            }}
          >
            <div>
              <h3 class="card-kicker">Start a subscription</h3>
              <p class="checkout-copy">
                Review the total, including tax, before confirming payment.
              </p>
            </div>
            <div class="checkout-fields">
              <div class="field-group">
                <label for={`checkout-cadence-${account.billingAccountId}`}>Billing period</label>
                <SelectControl>
                  <select
                    id={`checkout-cadence-${account.billingAccountId}`}
                    aria-label={`Billing period for ${accountName(account)}`}
                    disabled={draft.pending || draft.blockedUntil !== null}
                    value={draft.cadence}
                    onchange={(event) => updateCadence(
                      draft,
                      event.currentTarget.value as BillingCadence
                    )}
                  >
                    <option value="monthly">Monthly</option>
                    <option value="annual">Annual</option>
                  </select>
                </SelectControl>
              </div>
              {#if account.accountKind === 'organization'}
                <div class="field-group">
                  <label for={`checkout-seats-${account.billingAccountId}`}>Seats</label>
                  <input
                    id={`checkout-seats-${account.billingAccountId}`}
                    aria-label={`Seats for ${accountName(account)}`}
                    type="number"
                    min={Math.max(1, account.assignedSeatCount)}
                    step="1"
                    required
                    disabled={draft.pending || draft.blockedUntil !== null}
                    value={draft.seatQuantity}
                    oninput={(event) => updateSeatQuantity(draft, event.currentTarget.value)}
                  />
                </div>
              {:else}
                <p class="checkout-fixed-seat">One personal seat</p>
              {/if}
            </div>
            <p class="checkout-price" aria-live="polite">{priceLabel(draft)}</p>
            {#if draft.error}
              <p
                id={checkoutErrorId(account)}
                class="error-message"
                role="alert"
                tabindex="-1"
              >{draft.error}</p>
            {/if}
            <span class="visually-hidden" aria-live="polite">
              {draft.pending ? `Opening checkout for ${accountName(account)}` : ''}
            </span>
            <button
              class="primary-button"
              type="submit"
              aria-disabled={draft.pending || draft.blockedUntil !== null}
            >
              {#if draft.pending}
                Opening checkout…
              {:else if draft.blockedUntil}
                Wait for current Checkout to expire
              {:else if draft.billingOperationId}
                Try checkout again
              {:else}
                Continue to checkout
              {/if}
            </button>
          </form>
        {/if}
        {#if account.canManageBilling && account.subscription && customerPortalState}
          <div class="billing-management-stack">
            {#if !account.canStartCheckout && account.accountKind === 'organization' && seatQuantityDraft}
              <form
                class="checkout-form seat-quantity-form"
                aria-label={`Change seats for ${accountName(account)}`}
                aria-busy={seatQuantityDraft.pending}
                aria-describedby={seatQuantityDraft.error
                  ? seatQuantityErrorId(account)
                  : seatQuantityDraft.success
                    ? seatQuantitySuccessId(account)
                    : undefined}
                onsubmit={(event) => {
                  event.preventDefault();
                  void changeSubscriptionSeatQuantity(account, seatQuantityDraft);
                }}
              >
                <div>
                  <h3 class="card-kicker">Change organization seats</h3>
                  <p class="checkout-copy">
                    Added seats are prorated and invoiced now. Removing seats gives no
                    current-period credit and lowers the next renewal.
                  </p>
                </div>
                <div class="checkout-fields seat-quantity-fields">
                  <div class="field-group">
                    <label for={`subscription-seats-${account.billingAccountId}`}>
                      Purchased seats
                    </label>
                    <input
                      id={`subscription-seats-${account.billingAccountId}`}
                      aria-label={`Purchased seats for ${accountName(account)}`}
                      type="number"
                      min={Math.max(1, account.assignedSeatCount)}
                      max="2147483647"
                      step="1"
                      required
                      disabled={seatQuantityDraft.pending}
                      value={seatQuantityDraft.seatQuantity}
                      oninput={(event) => updateSubscriptionSeatQuantity(
                        seatQuantityDraft,
                        event.currentTarget.value
                      )}
                    />
                  </div>
                </div>
                {#if seatQuantityDraft.error}
                  <p
                    id={seatQuantityErrorId(account)}
                    class="error-message"
                    role="alert"
                    tabindex="-1"
                  >{seatQuantityDraft.error}</p>
                {/if}
                {#if seatQuantityDraft.success}
                  <p
                    id={seatQuantitySuccessId(account)}
                    class="success-message"
                    role="status"
                  >{seatQuantityDraft.success}</p>
                {/if}
                <span class="visually-hidden" aria-live="polite">
                  {seatQuantityDraft.pending
                    ? `Changing purchased seats for ${accountName(account)}`
                    : ''}
                </span>
                <button
                  class="primary-button"
                  type="submit"
                  aria-disabled={seatQuantityDraft.pending}
                >
                  {seatQuantityDraft.pending
                    ? 'Updating seats…'
                    : seatQuantityDraft.billingOperationId
                      ? 'Retry seat change'
                      : 'Update purchased seats'}
                </button>
              </form>
            {/if}

            <form
              class="checkout-form customer-portal-form"
              aria-label={`Manage billing for ${accountName(account)}`}
              aria-busy={customerPortalState.pending}
              aria-describedby={customerPortalState.error
                ? customerPortalErrorId(account)
                : undefined}
              onsubmit={(event) => {
                event.preventDefault();
                void openCustomerPortal(account, customerPortalState);
              }}
            >
              <div>
                <h3 class="card-kicker">Manage subscription</h3>
                <p class="checkout-copy">
                  Payment methods, invoices, and cancellation.
                </p>
              </div>
              {#if customerPortalState.error}
                <p
                  id={customerPortalErrorId(account)}
                  class="error-message"
                  role="alert"
                  tabindex="-1"
                >{customerPortalState.error}</p>
              {/if}
              <span class="visually-hidden" aria-live="polite">
                {customerPortalState.pending
                  ? `Opening billing for ${accountName(account)}`
                  : ''}
              </span>
              <button
                class="primary-button"
                type="submit"
                aria-disabled={customerPortalState.pending}
              >
                {customerPortalState.pending
                  ? 'Opening billing…'
                  : 'Manage billing'}
              </button>
            </form>
          </div>
        {:else if !account.canStartCheckout}
          <p class="billing-management-note">
            {#if account.canManageBilling}
              No billing changes are available for this plan.
            {:else}
              Only the owner can change billing.
            {/if}
          </p>
        {/if}
      </article>
    {/each}
  </div>
{/if}
