<script lang="ts">
  import { onMount, tick } from 'svelte';
  import SignInPanel from '$lib/components/SignInPanel.svelte';
  import {
    createPortalApi,
    PortalApiError,
    PortalAuthenticationRequiredError,
    portalErrorMessage,
    type OrganizationInvitation,
    type OrganizationMember,
    type OrganizationRole,
    type OrganizationSummary
  } from '$lib/api';
  import ActionMenu from '$lib/components/ActionMenu.svelte';
  import SelectControl from '$lib/components/SelectControl.svelte';
  import { formatUtcDateTime } from '$lib/date';

  const api = createPortalApi();

  function canManageOrganization(role: OrganizationRole | undefined) {
    return role === 'owner' || role === 'admin';
  }

  let organizations = $state<readonly OrganizationSummary[]>([]);
  let selectedOrganizationId = $state('');
  let members = $state<readonly OrganizationMember[]>([]);
  let invitations = $state<readonly OrganizationInvitation[]>([]);
  let loadingOrganizations = $state(true);
  let loadingWorkspace = $state(false);
  let authenticationRequired = $state(false);
  let pageError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let actionStatus = $state<string | null>(null);
  let failedAction = $state<string | null>(null);
  let invalidField = $state<'organization-name' | 'invitation-email' | null>(null);
  let busyAction = $state<string | null>(null);
  let organizationName = $state('');
  let invitationEmail = $state('');
  let invitationRole = $state<Exclude<OrganizationRole, 'owner'>>('member');
  let invitationProductSeat = $state(true);
  let actionStatusElement = $state<HTMLParagraphElement | null>(null);
  let workspaceLoad = 0;

  let selectedOrganization = $derived(
    organizations.find((organization) => organization.organizationId === selectedOrganizationId)
  );
  let canManageOrganizationAccess = $derived(
    canManageOrganization(selectedOrganization?.role)
  );
  let isOwner = $derived(selectedOrganization?.role === 'owner');
  let visibleEntryCount = $derived(members.length + invitations.length);

  onMount(() => {
    void loadOrganizations();
  });

  async function loadOrganizations(preferredOrganizationId?: string) {
    const load = ++workspaceLoad;
    loadingOrganizations = true;
    authenticationRequired = false;
    pageError = null;
    try {
      const loaded = await api.listOrganizations();
      if (load !== workspaceLoad) return;

      organizations = loaded;
      selectedOrganizationId =
        preferredOrganizationId && loaded.some((item) => item.organizationId === preferredOrganizationId)
          ? preferredOrganizationId
          : loaded.some((item) => item.organizationId === selectedOrganizationId)
            ? selectedOrganizationId
            : (loaded[0]?.organizationId ?? '');
      authenticationRequired = false;
      if (selectedOrganizationId) {
        await loadOrganizationWorkspace(selectedOrganizationId, load);
      } else {
        members = [];
        invitations = [];
      }
    } catch (error) {
      if (load === workspaceLoad) setPageError(error);
    } finally {
      if (load === workspaceLoad) loadingOrganizations = false;
    }
  }

  async function loadOrganizationWorkspace(organizationId: string, parentLoad?: number) {
    const load = parentLoad ?? ++workspaceLoad;
    loadingWorkspace = true;
    pageError = null;
    const organization = organizations.find((item) => item.organizationId === organizationId);
    try {
      const loadedMembers = await api.listMembers(organizationId);
      const loadedInvitations = canManageOrganization(organization?.role)
        ? await api.listInvitations(organizationId)
        : [];
      if (load !== workspaceLoad) return;

      members = loadedMembers;
      invitations = loadedInvitations;
      authenticationRequired = false;
    } catch (error) {
      if (load === workspaceLoad) setPageError(error);
    } finally {
      if (load === workspaceLoad) loadingWorkspace = false;
    }
  }

  function setPageError(error: unknown) {
    authenticationRequired = error instanceof PortalAuthenticationRequiredError;
    pageError = portalErrorMessage(error);
    members = [];
    invitations = [];
  }

  async function selectOrganization() {
    actionError = null;
    actionStatus = null;
    failedAction = null;
    invalidField = null;
    if (selectedOrganizationId) await loadOrganizationWorkspace(selectedOrganizationId);
  }

  async function createOrganization(event: SubmitEvent) {
    event.preventDefault();
    const name = organizationName.trim();
    if (!name) return;

    await runAction('create-organization', async () => {
      const created = await api.createOrganization(name);
      organizationName = '';
      await loadOrganizations(created.organizationId);
      actionStatus = created.trialWasTransferred
        ? `${name} is ready. Your active personal trial was transferred to it.`
        : `${name} is ready.`;
    });
  }

  async function changeRole(member: OrganizationMember, role: Exclude<OrganizationRole, 'owner'>) {
    if (!selectedOrganization || member.role === role) return;
    const organizationId = selectedOrganization.organizationId;

    await runAction(`role-${member.membershipId}`, async () => {
      await api.changeMemberRole(organizationId, member.membershipId, role);
      await loadOrganizationWorkspace(organizationId);
      actionStatus = `${member.email} is now ${role}.`;
    });
  }

  async function changeProductSeat(member: OrganizationMember, assigned: boolean) {
    if (!selectedOrganization || member.productSeatAssigned === assigned) return;
    if (
      !assigned &&
      !window.confirm(
        `Remove the license from ${member.email}? They will remain a member, but their devices will lose licensed access.`
      )
    ) {
      return;
    }

    const organizationId = selectedOrganization.organizationId;
    await runAction(`seat-${member.membershipId}`, async () => {
      await api.changeMemberSeat(organizationId, member.membershipId, assigned);
      await loadOrganizations(organizationId);
      actionStatus = assigned
        ? `A license was assigned to ${member.email}.`
        : `The license was removed from ${member.email}.`;
    });
  }

  async function transferOwnership(member: OrganizationMember) {
    if (!selectedOrganization) return;
    const organizationId = selectedOrganization.organizationId;
    const name = selectedOrganization.name;
    if (!window.confirm(`Transfer ownership of ${name} to ${member.email}?`)) {
      return;
    }

    await runAction(`transfer-${member.membershipId}`, async () => {
      await api.transferOwnership(organizationId, member.membershipId);
      await loadOrganizations(organizationId);
      actionStatus = `${member.email} is now the organization owner.`;
    });
  }

  async function removeMember(member: OrganizationMember) {
    if (!selectedOrganization) return;
    const organizationId = selectedOrganization.organizationId;
    const name = selectedOrganization.name;
    const isCurrentUser = member.membershipId === selectedOrganization.membershipId;
    const prompt = isCurrentUser
      ? `Leave ${name}? Your membership and saved devices will be removed.`
      : `Remove ${member.email}? Their membership and saved devices will be removed.`;
    if (!window.confirm(prompt)) return;

    await runAction(`remove-${member.membershipId}`, async () => {
      await api.removeMember(organizationId, member.membershipId);
      await loadOrganizations(organizationId);
      actionStatus = isCurrentUser
        ? `You left ${name}.`
        : `${member.email} was removed.`;
    });
  }

  async function createInvitation(event: SubmitEvent) {
    event.preventDefault();
    if (!selectedOrganization) return;
    const organizationId = selectedOrganization.organizationId;
    const email = invitationEmail.trim();
    if (!email) return;

    await runAction('create-invitation', async () => {
      await api.createInvitation(
        organizationId,
        email,
        invitationRole,
        invitationProductSeat
      );
      invitationEmail = '';
      invitationProductSeat = true;
      await loadOrganizationWorkspace(organizationId);
      actionStatus = `Invitation sent to ${email}.`;
    });
  }

  async function resendInvitation(invitation: OrganizationInvitation) {
    if (!selectedOrganization) return;
    const organizationId = selectedOrganization.organizationId;
    await runAction(`resend-${invitation.invitationId}`, async () => {
      await api.resendInvitation(organizationId, invitation.invitationId);
      await loadOrganizationWorkspace(organizationId);
      actionStatus = `A fresh invitation was sent to ${invitation.email}.`;
    });
  }

  async function cancelInvitation(invitation: OrganizationInvitation) {
    if (!selectedOrganization) return;
    const organizationId = selectedOrganization.organizationId;
    if (!window.confirm(`Cancel the invitation for ${invitation.email}?`)) return;

    await runAction(`cancel-${invitation.invitationId}`, async () => {
      await api.cancelInvitation(organizationId, invitation.invitationId);
      await loadOrganizationWorkspace(organizationId);
      actionStatus = `The invitation for ${invitation.email} was cancelled.`;
    });
  }

  async function runAction(key: string, action: () => Promise<void>) {
    if (busyAction) return;
    busyAction = key;
    actionError = null;
    actionStatus = null;
    failedAction = null;
    invalidField = null;
    try {
      await action();
      if (actionStatus) {
        await tick();
        actionStatusElement?.focus();
      }
    } catch (error) {
      const message = portalErrorMessage(error);
      if (error instanceof PortalAuthenticationRequiredError) {
        authenticationRequired = true;
        pageError = message;
        actionError = null;
      } else {
        actionError = message;
        failedAction = key;
        invalidField = validationFieldFor(key, error);
      }
    } finally {
      busyAction = null;
    }
  }

  function titleCase(value: string) {
    return value.charAt(0).toUpperCase() + value.slice(1);
  }

  function canRemoveMember(member: OrganizationMember) {
    if (!selectedOrganization || member.role === 'owner') return false;
    if (member.membershipId === selectedOrganization.membershipId) return true;
    if (selectedOrganization.role === 'owner') return true;
    return selectedOrganization.role === 'admin' && member.role === 'member';
  }

  function closeActionMenu(event: MouseEvent) {
    (event.currentTarget as HTMLElement).closest<HTMLElement>('[popover]')?.hidePopover();
  }

  function validationFieldFor(
    action: string,
    error: unknown
  ): 'organization-name' | 'invitation-email' | null {
    if (!(error instanceof PortalApiError)) return null;
    if (action === 'create-organization' && error.validationErrors.name?.length) {
      return 'organization-name';
    }
    if (action === 'create-invitation' && error.validationErrors.email?.length) {
      return 'invitation-email';
    }
    return null;
  }

  function clearFieldError(field: 'organization-name' | 'invitation-email') {
    if (invalidField !== field) return;
    invalidField = null;
    failedAction = null;
    actionError = null;
  }

  function organizationCreationDescription() {
    const descriptions: string[] = [];
    if (!loadingOrganizations && organizations.length === 0) {
      descriptions.push('organization-trial-transfer-help');
    }
    if (failedAction === 'create-organization') {
      descriptions.push('organization-action-error');
    }
    return descriptions.length > 0 ? descriptions.join(' ') : undefined;
  }
</script>

<svelte:head>
  <title>Organizations · Styrhous</title>
</svelte:head>

<section class="page-heading" aria-labelledby="organizations-heading">
  <h1 id="organizations-heading">Organizations</h1>
</section>

{#if authenticationRequired}
  <SignInPanel
    title="Sign in to manage organizations"
    message={pageError ?? 'Sign in to continue.'}
    onretry={() => void loadOrganizations()}
  />
{:else}
  <section class="workspace-toolbar" aria-label="Organization controls">

    {#if organizations.length > 0}
      <div class="field-group organization-switcher">
        <label for="organization-switcher">Current organization</label>
        <SelectControl>
          <select
            id="organization-switcher"
            bind:value={selectedOrganizationId}
            onchange={() => void selectOrganization()}
            disabled={loadingOrganizations || busyAction !== null}
          >
            {#each organizations as organization}
              <option value={organization.organizationId}>{organization.name}</option>
            {/each}
          </select>
        </SelectControl>
      </div>
    {/if}
    <details class="organization-create" open={!loadingOrganizations && organizations.length === 0}>
      <summary>New organization</summary>
    <form
      class="compact-form"
      aria-describedby={organizationCreationDescription()}
      onsubmit={createOrganization}
    >
      {#if !loadingOrganizations && organizations.length === 0}
        <p id="organization-trial-transfer-help" class="organization-creation-note">
          If this is the first organization you create and your personal trial is active, the
          trial moves to the organization.
        </p>
      {/if}
      <div class="field-group grow-field">
        <label for="organization-name">New organization</label>
        <input
          id="organization-name"
          name="organizationName"
          bind:value={organizationName}
          maxlength="120"
          placeholder="Platform team"
          autocomplete="organization"
          aria-invalid={invalidField === 'organization-name' ? 'true' : undefined}
          aria-describedby={organizationCreationDescription()}
          oninput={() => clearFieldError('organization-name')}
          required
        />
      </div>
      <button class="primary-button" type="submit" disabled={busyAction !== null}>
        {busyAction === 'create-organization' ? 'Creating…' : 'Create organization'}
      </button>
    </form>

    </details>
  </section>

  <div class="message-stack" aria-live="polite">
    {#if actionStatus}
      <p bind:this={actionStatusElement} class="success-message" role="status" tabindex="-1">
        {actionStatus}
      </p>
    {/if}
    {#if actionError}<p id="organization-action-error" class="error-message" role="alert">{actionError}</p>{/if}
  </div>

  {#if loadingOrganizations}
    <section class="state-panel" aria-busy="true" aria-label="Loading organizations">
      <span class="state-mark" aria-hidden="true">•••</span>
      <div><h2>Loading organizations</h2><p>Loading memberships and seats.</p></div>
    </section>
  {:else if pageError}
    <section class="state-panel error-panel" aria-labelledby="organization-error-heading">
      <span class="state-mark" aria-hidden="true">!</span>
      <div>
        <h2 id="organization-error-heading">Organizations could not be loaded</h2>
        <p role="alert">{pageError}</p>
        <button class="secondary-action" type="button" onclick={() => void loadOrganizations()}>
          Try again
        </button>
      </div>
    </section>
  {:else if organizations.length === 0}
    <section class="state-panel" aria-labelledby="empty-organizations-heading">
      <span class="state-mark" aria-hidden="true">—</span>
      <div>
        <p class="card-kicker">Current state</p>
        <h2 id="empty-organizations-heading">No organizations</h2>
        <p>Create an organization above to manage shared seats and members.</p>
      </div>
    </section>
  {:else if selectedOrganization}
    <section
      class="workspace-panel organization-seats-panel"
      aria-labelledby="organization-seats-heading"
      aria-busy={loadingWorkspace}
    >
      <div class="section-heading-row">
        <div>
          <h2 id="organization-seats-heading">{selectedOrganization.name}</h2>
          <p class="muted-copy">{visibleEntryCount} members and invitations · {titleCase(selectedOrganization.role)}</p>
          {#if !selectedOrganization.productSeatAssigned}<p class="muted-copy">Organization access only. No license assigned.</p>{/if}
        </div>

      </div>

      {#if canManageOrganizationAccess}
        <details class="invitation-disclosure">
          <summary>Invite people</summary>
        <form
          class="invite-form"
          aria-describedby={failedAction === 'create-invitation'
            ? 'organization-action-error'
            : undefined}
          onsubmit={createInvitation}
        >
          <div class="field-group grow-field">
            <label for="invitation-email">Email address</label>
            <input
              id="invitation-email"
              name="email"
              type="email"
              bind:value={invitationEmail}
              autocomplete="email"
              placeholder="teammate@example.com"
              aria-invalid={invalidField === 'invitation-email' ? 'true' : undefined}
              aria-describedby={invalidField === 'invitation-email'
                ? 'organization-action-error'
                : undefined}
              oninput={() => clearFieldError('invitation-email')}
              required
            />
          </div>
          <div class="field-group">
            <label for="invitation-role">Role</label>
            <SelectControl>
              <select id="invitation-role" bind:value={invitationRole}>
                <option value="member">Member</option>
                <option value="admin">Admin</option>
              </select>
            </SelectControl>
          </div>
          <label class="checkbox-field" for="invitation-product-seat">
            <input
              id="invitation-product-seat"
              type="checkbox"
              bind:checked={invitationProductSeat}
            />
            <span>
              <strong>Include license</strong>
              <small>Allows this person to use Styrhous.</small>
            </span>
          </label>
          <button class="primary-button" type="submit" disabled={busyAction !== null}>
            {busyAction === 'create-invitation' ? 'Sending…' : 'Send invitation'}
          </button>
        </form>
        </details>
      {/if}

      {#if loadingWorkspace}
        <p class="muted-copy">Loading organization members…</p>
      {:else}
        <div class="organization-seat-table-scroll">
          <table class="organization-seat-table">
            <caption class="visually-hidden">Organization members and invitations</caption>
            <thead>
              <tr>
                <th role="columnheader" scope="col">Person</th>
                <th role="columnheader" scope="col">Status</th>
                <th role="columnheader" scope="col">Role</th>
                <th role="columnheader" scope="col">License</th>
                <th scope="col" abbr="Device limit">Devices</th>
                <th role="columnheader" scope="col">Activity</th>
                <th scope="col" class="organization-seat-actions-heading">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each members as member (member.membershipId)}
                <tr>
                  <th role="rowheader" scope="row">
                    <div class="organization-seat-person">
                      <strong>{member.email}</strong>
                      {#if member.membershipId === selectedOrganization.membershipId}
                        <span class="current-user-label">You</span>
                      {/if}
                    </div>
                  </th>
                  <td role="cell" data-label="Status"><span class="state-badge eligible">Active</span></td>
                  <td role="cell" data-label="Role" class="organization-seat-role">
                    {#if member.role === 'owner'}
                      <span class="role-badge">Owner</span>
                    {:else if isOwner}
                      <label class="visually-hidden" for={`role-${member.membershipId}`}>
                        Role for {member.email}
                      </label>
                      <SelectControl>
                        <select
                          class="inline-select"
                          id={`role-${member.membershipId}`}
                          value={member.role}
                          disabled={busyAction !== null}
                          onchange={(event) => {
                            const role = event.currentTarget.value as Exclude<OrganizationRole, 'owner'>;
                            event.currentTarget.value = member.role;
                            void changeRole(member, role);
                          }}
                        >
                          <option value="admin">Admin</option>
                          <option value="member">Member</option>
                        </select>
                      </SelectControl>
                    {:else}
                      <span class="role-badge subtle">{titleCase(member.role)}</span>
                    {/if}
                  </td>
                  <td role="cell" data-label="License" class="organization-seat-assignment">
                    {#if canManageOrganizationAccess}
                      <label class="visually-hidden" for={`seat-${member.membershipId}`}>
                        License for {member.email}
                      </label>
                      <SelectControl>
                        <select
                          class="inline-select"
                          id={`seat-${member.membershipId}`}
                          value={member.productSeatAssigned ? 'assigned' : 'unassigned'}
                          disabled={busyAction !== null}
                          onchange={(event) => {
                            const assigned = event.currentTarget.value === 'assigned';
                            event.currentTarget.value = member.productSeatAssigned
                              ? 'assigned'
                              : 'unassigned';
                            void changeProductSeat(member, assigned);
                          }}
                        >
                          <option value="assigned">Assigned</option>
                          <option value="unassigned">Not assigned</option>
                        </select>
                      </SelectControl>
                    {:else}
                      <span class:eligible={member.productSeatAssigned} class="state-badge">
                        {member.productSeatAssigned ? 'Assigned' : 'Not assigned'}
                      </span>
                    {/if}
                  </td>
                  <td role="cell" data-label="Device limit">
                    {#if member.productSeatAssigned}
                      {member.deviceLimit}
                    {:else}
                      <span class="organization-seat-no-value" aria-hidden="true">—</span>
                      <span class="visually-hidden">No license assigned</span>
                    {/if}
                  </td>
                  <td role="cell" data-label="Activity" class="organization-seat-activity-cell">
                    <div class="organization-seat-activity">
                      <span>Joined</span>
                      <time datetime={member.joinedAt}>{formatUtcDateTime(member.joinedAt)}</time>
                    </div>
                  </td>
                  <td role="cell" data-label="Actions" class="organization-seat-actions-cell">
                    {#if (isOwner && member.role !== 'owner') || canRemoveMember(member)}
                      <ActionMenu
                        id={`member-actions-${member.membershipId}`}
                        subject={member.email}
                        disabled={busyAction !== null}
                      >
                        {#if isOwner && member.membershipId !== selectedOrganization.membershipId}
                          <button
                            class="organization-seat-menu-item"
                            type="button"
                            disabled={busyAction !== null}
                            aria-label={`Make ${member.email} owner`}
                            onclick={(event) => {
                              closeActionMenu(event);
                              void transferOwnership(member);
                            }}
                          >
                            {busyAction === `transfer-${member.membershipId}`
                              ? 'Transferring…'
                              : 'Make owner'}
                          </button>
                        {/if}
                        {#if canRemoveMember(member)}
                          <button
                            class="organization-seat-menu-item danger"
                            type="button"
                            disabled={busyAction !== null}
                            aria-label={member.membershipId === selectedOrganization.membershipId
                              ? `Leave ${selectedOrganization.name}`
                              : `Remove ${member.email}`}
                            onclick={(event) => {
                              closeActionMenu(event);
                              void removeMember(member);
                            }}
                          >
                            {busyAction === `remove-${member.membershipId}`
                              ? 'Removing…'
                              : member.membershipId === selectedOrganization.membershipId
                                ? 'Leave organization'
                                : 'Remove member'}
                          </button>
                        {/if}
                      </ActionMenu>
                    {:else}
                      <span class="organization-seat-no-value" aria-hidden="true">—</span>
                      <span class="visually-hidden">No available actions</span>
                    {/if}
                  </td>
                </tr>
              {/each}
              {#each invitations as invitation (invitation.invitationId)}
                <tr>
                  <th role="rowheader" scope="row"><strong>{invitation.email}</strong></th>
                  <td role="cell" data-label="Status"><span class="state-badge">Pending invitation</span></td>
                  <td role="cell" data-label="Role">{titleCase(invitation.role)}</td>
                  <td role="cell" data-label="License">
                    <span class:eligible={invitation.assignProductSeat} class="state-badge">
                      {invitation.assignProductSeat ? 'On acceptance' : 'Not included'}
                    </span>
                  </td>
                  <td role="cell" data-label="Device limit">
                    <span class="organization-seat-no-value" aria-hidden="true">—</span>
                    <span class="visually-hidden">
                      {invitation.assignProductSeat
                        ? 'Available after the invitation is accepted'
                        : 'No license included'}
                    </span>
                  </td>
                  <td role="cell" data-label="Activity" class="organization-seat-activity-cell">
                    <div class="organization-seat-activity">
                      <span>
                        Last sent
                        <time datetime={invitation.lastSentAt}
                          >{formatUtcDateTime(invitation.lastSentAt)}</time
                        >
                      </span>
                      <span>
                        Expires
                        <time datetime={invitation.expiresAt}
                          >{formatUtcDateTime(invitation.expiresAt)}</time
                        >
                      </span>
                    </div>
                  </td>
                  <td role="cell" data-label="Actions" class="organization-seat-actions-cell">
                    <ActionMenu
                      id={`invitation-actions-${invitation.invitationId}`}
                      subject={invitation.email}
                      disabled={busyAction !== null}
                    >
                      <button
                        class="organization-seat-menu-item"
                        type="button"
                        disabled={busyAction !== null}
                        aria-label={`Resend invitation to ${invitation.email}`}
                        onclick={(event) => {
                          closeActionMenu(event);
                          void resendInvitation(invitation);
                        }}
                      >
                        {busyAction === `resend-${invitation.invitationId}`
                          ? 'Sending…'
                          : 'Resend'}
                      </button>
                      <button
                        class="organization-seat-menu-item danger"
                        type="button"
                        disabled={busyAction !== null}
                        aria-label={`Cancel invitation to ${invitation.email}`}
                        onclick={(event) => {
                          closeActionMenu(event);
                          void cancelInvitation(invitation);
                        }}
                      >
                        {busyAction === `cancel-${invitation.invitationId}`
                          ? 'Cancelling…'
                          : 'Cancel'}
                      </button>
                    </ActionMenu>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  {/if}
{/if}
