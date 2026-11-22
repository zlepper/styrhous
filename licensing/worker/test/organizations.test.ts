import { env } from 'cloudflare:workers';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { countRows, past } from './support/db.ts';
import { desktop, signInDesktop } from './support/desktop.ts';
import { completeOAuth, signInAsGitHubAccount, signedInBrowser } from './support/identity.ts';
import { addMember, captureEmails, createOrganization, secretFrom } from './support/organizations.ts';
import { Browser } from './support/portal.ts';
import { FakeUpstream } from './support/upstream.ts';

let upstream: FakeUpstream;

beforeEach(() => {
  upstream = new FakeUpstream().install();
});

async function expireOrganizationTrial(billingAccountId: string) {
  await env.DB.prepare('UPDATE trials SET started_at = ?, ends_at = ? WHERE billing_account_id = ?')
    .bind(past, '2020-01-02T00:00:00.000Z', billingAccountId)
    .run();
}

describe('organizations', () => {
  it("starts a trial with the user's first organization only", async () => {
    const { browser, userId } = await signedInBrowser(upstream, 'founder@example.com');
    const first = await browser.json('/api/organizations', 'POST', { name: '  First  ' });
    expect(first.body).toMatchObject({ trialStarted: true });
    const second = await browser.json('/api/organizations', 'POST', { name: 'Second' });
    expect(second.body).toMatchObject({ trialStarted: false });

    const organizations = await browser.json<{ organizations: { name: string; role: string }[] }>('/api/organizations');
    expect(organizations.body.organizations.map((organization) => [organization.name, organization.role])).toEqual([
      ['First', 'owner'],
      ['Second', 'owner']
    ]);
    const trialsOf = 'SELECT count(*) AS total FROM trials WHERE billing_account_id = ?';
    expect(await countRows(trialsOf, first.body.billingAccountId)).toBe(1);
    expect(await countRows(trialsOf, second.body.billingAccountId)).toBe(0);
    const starts = "SELECT count(*) AS total FROM audit_records WHERE action = 'trial_started' AND actor_user_id = ?";
    expect(await countRows(starts, userId)).toBe(1);

    const entitlements = await browser.json<{ entitlements: Record<string, unknown>[] }>('/api/entitlements');
    expect(
      entitlements.body.entitlements.map((entitlement) => [entitlement.billingAccountId, entitlement.reasonCode])
    ).toEqual(
      expect.arrayContaining([
        [first.body.billingAccountId, 'active_trial'],
        [second.body.billingAccountId, 'no_valid_entitlement']
      ])
    );
  });

  it('starts only one trial when a user creates two organizations at once', async () => {
    const { browser, userId } = await signedInBrowser(upstream, 'racing-founder@example.com');
    const results = await Promise.all(
      ['Alpha', 'Beta'].map((name) =>
        browser.json<{ billingAccountId: string; trialStarted: boolean }>('/api/organizations', 'POST', { name })
      )
    );

    expect(results.map((result) => result.status)).toEqual([201, 201]);
    const started = results.filter((result) => result.body.trialStarted);
    expect(started).toHaveLength(1);
    const trialAccount = await env.DB.prepare('SELECT billing_account_id AS id FROM trials WHERE originating_user_id = ?')
      .bind(userId)
      .all<{ id: string }>();
    expect(trialAccount.results.map((row) => row.id)).toEqual([started[0]!.body.billingAccountId]);
    const starts = "SELECT count(*) AS total FROM audit_records WHERE action = 'trial_started' AND actor_user_id = ?";
    expect(await countRows(starts, userId)).toBe(1);
  });

  it('validates the organization name', async () => {
    const { browser } = await signedInBrowser(upstream, 'validator@example.com');
    const response = await browser.json('/api/organizations', 'POST', { name: '   ' });
    expect(response.status).toBe(400);
    expect(response.body).toMatchObject({ errors: { name: [expect.any(String)] } });
  });

  it('hides organizations from non-members', async () => {
    const owner = await signedInBrowser(upstream, 'hidden-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const outsider = await signedInBrowser(upstream, 'outsider@example.com');
    expect(await outsider.browser.json(`/api/organizations/${organization.organizationId}/members`)).toEqual({
      status: 404,
      body: { reasonCode: 'organization_not_found' }
    });
  });
});

describe('roles and membership', () => {
  it('lets only the owner change roles and transfer ownership', async () => {
    const owner = await signedInBrowser(upstream, 'role-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const admin = await addMember(upstream, owner.browser, organization, 'role-admin@example.com', { role: 'admin' });
    const member = await addMember(upstream, owner.browser, organization, 'role-member@example.com');
    const base = `/api/organizations/${organization.organizationId}/members`;

    expect((await admin.browser.json(`${base}/${member.membershipId}/role`, 'PATCH', { role: 'admin' })).body).toEqual({
      reasonCode: 'insufficient_permission'
    });
    expect((await owner.browser.json(`${base}/${member.membershipId}/role`, 'PATCH', { role: 'member' })).body).toEqual({
      reasonCode: 'organization_member_role_unchanged'
    });
    expect(
      (await owner.browser.json(`${base}/${organization.ownerMembershipId}/role`, 'PATCH', { role: 'admin' })).body
    ).toEqual({ reasonCode: 'ownership_transfer_required' });

    const transfer = await owner.browser.json(`${base}/${member.membershipId}/transfer-ownership`, 'POST');
    expect(transfer.body).toMatchObject({ reasonCode: 'organization_ownership_transferred' });
    const roles = await owner.browser.json<{ members: { membershipId: string; role: string }[] }>(base);
    expect(Object.fromEntries(roles.body.members.map((row) => [row.membershipId, row.role]))).toEqual({
      [organization.ownerMembershipId]: 'admin',
      [admin.membershipId]: 'admin',
      [member.membershipId]: 'owner'
    });
  });

  it('applies the member removal policy and frees the seat', async () => {
    const owner = await signedInBrowser(upstream, 'removal-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const admin = await addMember(upstream, owner.browser, organization, 'removal-admin@example.com', { role: 'admin' });
    const member = await addMember(upstream, owner.browser, organization, 'removal-member@example.com');
    const base = `/api/organizations/${organization.organizationId}/members`;

    expect((await admin.browser.json(`${base}/${organization.ownerMembershipId}`, 'DELETE')).body).toEqual({
      reasonCode: 'ownership_transfer_required'
    });
    expect((await member.browser.json(`${base}/${admin.membershipId}`, 'DELETE')).body).toEqual({
      reasonCode: 'insufficient_permission'
    });
    expect((await admin.browser.json(`${base}/${member.membershipId}`, 'DELETE')).body).toMatchObject({
      reasonCode: 'organization_member_removed'
    });
    expect((await admin.browser.json(`${base}/${admin.membershipId}`, 'DELETE')).status).toBe(200);

    const remaining = await owner.browser.json<{ members: unknown[] }>(base);
    expect(remaining.body.members).toHaveLength(1);
    expect(await countRows('SELECT count(*) AS total FROM seats WHERE billing_account_id = ?', organization.billingAccountId)).toBe(1);
  });

  it('signs a removed member out of their desktop installations', async () => {
    const owner = await signedInBrowser(upstream, 'desktop-removal-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const member = await addMember(upstream, owner.browser, organization, 'desktop-removal-member@example.com');
    const installation = await signInDesktop(member.browser, undefined, member.seatId);
    expect((await desktop('/entitlement', { token: installation.token })).status).toBe(200);

    const removal = await owner.browser.json(`/api/organizations/${organization.organizationId}/members/${member.membershipId}`, 'DELETE');
    expect(removal.status).toBe(200);
    expect((await desktop('/entitlement', { token: installation.token })).status).toBe(401);
    expect(await countRows('SELECT count(*) AS total FROM device_activations WHERE seat_id = ?', member.seatId)).toBe(0);
  });

  it('lets only one of two concurrent removals of a member succeed', async () => {
    const owner = await signedInBrowser(upstream, 'double-removal-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const member = await addMember(upstream, owner.browser, organization, 'double-removal-member@example.com');
    const path = `/api/organizations/${organization.organizationId}/members/${member.membershipId}`;

    const results = await Promise.all([1, 2].map(() => owner.browser.json(path, 'DELETE')));
    const [removed, rejected] = [...results].sort((a, b) => a.status - b.status);
    expect(removed!.status).toBe(200);
    expect(rejected!.body.reasonCode).toMatch(/^(organization_member_not_found|organization_members_changed)$/);
  });

  it('reports why a seat or ownership change was refused', async () => {
    const owner = await signedInBrowser(upstream, 'codes-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const admin = await addMember(upstream, owner.browser, organization, 'codes-admin@example.com', { role: 'admin' });
    const member = await addMember(upstream, owner.browser, organization, 'codes-member@example.com');
    const base = `/api/organizations/${organization.organizationId}/members`;

    expect((await owner.browser.json(`${base}/${member.membershipId}/seat`, 'PATCH', { assigned: false })).body).toMatchObject({
      reasonCode: 'organization_member_seat_unassigned',
      assigned: false
    });
    expect(await owner.browser.json(`${base}/${member.membershipId}/seat`, 'PATCH', { assigned: false })).toEqual({
      status: 409,
      body: { reasonCode: 'organization_member_seat_unchanged' }
    });
    expect(await member.browser.json(`${base}/${member.membershipId}/seat`, 'PATCH', { assigned: true })).toEqual({
      status: 403,
      body: { reasonCode: 'insufficient_permission' }
    });
    expect(await admin.browser.json(`${base}/${member.membershipId}/transfer-ownership`, 'POST')).toEqual({
      status: 403,
      body: { reasonCode: 'insufficient_permission' }
    });
    expect(await owner.browser.json(`${base}/${organization.ownerMembershipId}/transfer-ownership`, 'POST')).toEqual({
      status: 409,
      body: { reasonCode: 'ownership_transfer_target_invalid' }
    });

    const other = await createOrganization(owner.browser, 'Other');
    for (const membershipId of [other.ownerMembershipId, 'not-a-uuid']) {
      expect(await owner.browser.json(`${base}/${membershipId}/role`, 'PATCH', { role: 'admin' })).toEqual({
        status: 404,
        body: { reasonCode: 'organization_member_not_found' }
      });
    }
  });
});

describe('seat capacity', () => {
  it('lets exactly one of two concurrent requests take the last seat', async () => {
    const owner = await signedInBrowser(upstream, 'capacity-owner@example.com');
    const organization = await createOrganization(owner.browser);
    // The organization's trial provides five seats; the owner holds one.
    const members = [];
    for (const index of [1, 2, 3, 4, 5]) {
      members.push(
        await addMember(upstream, owner.browser, organization, `capacity-${index}@example.com`, { assignProductSeat: index <= 3 })
      );
    }
    const base = `/api/organizations/${organization.organizationId}/members`;

    const results = await Promise.all(
      members.slice(3).map((member) => owner.browser.json(`${base}/${member.membershipId}/seat`, 'PATCH', { assigned: true }))
    );
    expect(results.map((result) => result.status).sort()).toEqual([200, 409]);
    expect(results.find((result) => result.status === 409)!.body).toEqual({ reasonCode: 'seat_capacity_reached' });

    const invitation = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'one-too-many@example.com',
      role: 'member'
    });
    expect(invitation.body).toEqual({ reasonCode: 'seat_capacity_reached' });
  });

  it('reports missing capacity once the trial is over', async () => {
    const owner = await signedInBrowser(upstream, 'expired-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const sent = captureEmails();
    const pending = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'before-expiry@example.com',
      role: 'member'
    });
    expect(pending.status).toBe(201);
    await expireOrganizationTrial(organization.billingAccountId);

    const invitee = await signedInBrowser(upstream, 'before-expiry@example.com');
    expect(await invitee.browser.json('/api/invitations/accept', 'POST', { secret: secretFrom(sent.mock.calls[0]![0]) })).toEqual({
      status: 409,
      body: { reasonCode: 'no_active_seat_capacity' }
    });
    const invitation = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'late@example.com',
      role: 'member'
    });
    expect(invitation.body).toEqual({ reasonCode: 'no_active_seat_capacity' });
  });
});

describe('invitations', () => {
  it('emails a secret link that only the invited address can accept', async () => {
    const owner = await signedInBrowser(upstream, 'invite-owner@example.com');
    const organization = await createOrganization(owner.browser, 'Invite & Co');
    const sent = captureEmails();
    const created = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'Invitee@Example.com',
      role: 'admin',
      assignProductSeat: false
    });
    expect(created.body).toMatchObject({ reasonCode: 'invitation_created', emailDelivered: true });
    const message = sent.mock.calls[0]![0] as { to: string; subject: string; text: string; html: string };
    expect(message.to).toBe('Invitee@Example.com');
    expect(message.subject).toContain('Invite & Co');
    expect(message.html).toContain('Invite &amp; Co');

    const duplicate = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'invitee@example.com',
      role: 'member'
    });
    expect(duplicate.body).toEqual({ reasonCode: 'invitation_already_pending' });

    const stranger = await signedInBrowser(upstream, 'stranger@example.com');
    expect(await stranger.browser.json('/api/invitations/accept', 'POST', { secret: secretFrom(message) })).toEqual({
      status: 403,
      body: { reasonCode: 'invitation_email_mismatch' }
    });

    const invitee = await signedInBrowser(upstream, 'invitee@example.com');
    const accepted = await invitee.browser.json('/api/invitations/accept', 'POST', { secret: secretFrom(message) });
    expect(accepted.body).toMatchObject({ reasonCode: 'invitation_accepted', role: 'admin', productSeatAssigned: false });
    expect((await invitee.browser.json('/api/invitations/accept', 'POST', { secret: secretFrom(message) })).status).toBe(404);

    const again = await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'INVITEE@example.com',
      role: 'member'
    });
    expect(again.body).toEqual({ reasonCode: 'already_member' });
  });

  it('keeps the invitation when email delivery fails and replaces the secret on resend', async () => {
    const owner = await signedInBrowser(upstream, 'resend-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const base = `/api/organizations/${organization.organizationId}/invitations`;
    const sent = vi.spyOn(env.EMAIL, 'send').mockRejectedValueOnce(new Error('delivery failed'));

    const created = await owner.browser.json<{ invitationId: string; emailDelivered: boolean }>(base, 'POST', {
      email: 'resend@example.com',
      role: 'member'
    });
    expect(created.status).toBe(201);
    expect(created.body.emailDelivered).toBe(false);
    const listed = await owner.browser.json<{ invitations: { emailDelivered: boolean }[] }>(base);
    expect(listed.body.invitations).toEqual([expect.objectContaining({ emailDelivered: false })]);

    sent.mockResolvedValue({ messageId: 'resent' });
    const resent = await owner.browser.json(`${base}/${created.body.invitationId}/resend`, 'POST');
    expect(resent.body).toMatchObject({ reasonCode: 'invitation_resent', emailDelivered: true });
    const secret = secretFrom(sent.mock.calls.at(-1)![0] as { text: string });

    const cancelled = await owner.browser.json(`${base}/${created.body.invitationId}`, 'DELETE');
    expect(cancelled.body).toMatchObject({ reasonCode: 'invitation_cancelled' });
    const invitee = await signedInBrowser(upstream, 'resend@example.com');
    expect((await invitee.browser.json('/api/invitations/accept', 'POST', { secret })).status).toBe(404);
  });

  it('refuses expired, cancelled, and unknown invitations', async () => {
    const owner = await signedInBrowser(upstream, 'edge-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const base = `/api/organizations/${organization.organizationId}/invitations`;
    const sent = captureEmails();
    const expiring = await owner.browser.json<{ invitationId: string }>(base, 'POST', { email: 'edge-expired@example.com', role: 'member' });
    await env.DB.prepare('UPDATE invitations SET expires_at = ? WHERE id = ?').bind(past, expiring.body.invitationId).run();
    const invitee = await signedInBrowser(upstream, 'edge-expired@example.com');
    expect(await invitee.browser.json('/api/invitations/accept', 'POST', { secret: secretFrom(sent.mock.calls[0]![0]) })).toEqual({
      status: 404,
      body: { reasonCode: 'invitation_not_found' }
    });

    const cancelled = await owner.browser.json<{ invitationId: string }>(base, 'POST', { email: 'edge-cancelled@example.com', role: 'member' });
    expect((await owner.browser.json(`${base}/${cancelled.body.invitationId}`, 'DELETE')).status).toBe(200);
    const notFound = { status: 404, body: { reasonCode: 'invitation_not_found' } };
    expect(await owner.browser.json(`${base}/${cancelled.body.invitationId}`, 'DELETE')).toEqual(notFound);
    expect(await owner.browser.json(`${base}/${cancelled.body.invitationId}/resend`, 'POST')).toEqual(notFound);
    expect(await owner.browser.json(`${base}/not-a-uuid/resend`, 'POST')).toEqual(notFound);
  });

  it('recognizes members by any of their verified emails', async () => {
    const owner = await signedInBrowser(upstream, 'aliases-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const member = await addMember(upstream, owner.browser, organization, 'aliases-old@example.com');
    // The member's GitHub account now reports a new address, which becomes verified too.
    signInAsGitHubAccount(upstream, { ...member.account, email: 'aliases-new@example.com' });
    await completeOAuth(new Browser(), '/auth/sign-in/github');

    captureEmails();
    for (const email of ['aliases-old@example.com', 'aliases-new@example.com']) {
      expect((await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', { email, role: 'member' })).body).toEqual({
        reasonCode: 'already_member'
      });
    }
  });

  it('still starts a trial for a member who later creates their own organization', async () => {
    const owner = await signedInBrowser(upstream, 'trial-host@example.com');
    const hosting = await createOrganization(owner.browser, 'Hosting');
    const member = await addMember(upstream, owner.browser, hosting, 'trial-guest@example.com');

    const own = await member.browser.json('/api/organizations', 'POST', { name: 'Guest Labs' });
    expect(own.body).toMatchObject({ trialStarted: true });

    const trialOf = 'SELECT originating_user_id AS id FROM trials WHERE billing_account_id = ?';
    const trialOwner = async (billingAccountId: string) =>
      (await env.DB.prepare(trialOf).bind(billingAccountId).first<{ id: string }>())?.id;
    expect(await trialOwner(hosting.billingAccountId)).toBe(owner.userId);
    expect(await trialOwner(own.body.billingAccountId)).toBe(member.userId);
  });

  it('accepts an invitation only once when it is accepted twice at once', async () => {
    const owner = await signedInBrowser(upstream, 'double-accept-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const sent = captureEmails();
    await owner.browser.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
      email: 'double-accept@example.com',
      role: 'member'
    });
    const invitee = await signedInBrowser(upstream, 'double-accept@example.com');
    const secret = secretFrom(sent.mock.calls[0]![0]);

    const results = await Promise.all([1, 2].map(() => invitee.browser.json('/api/invitations/accept', 'POST', { secret })));
    const [accepted, rejected] = [...results].sort((a, b) => a.status - b.status);
    expect(accepted!.status).toBe(200);
    expect(rejected!.body.reasonCode).toMatch(/^(already_member|invitation_not_found)$/);
    expect(
      await countRows('SELECT count(*) AS total FROM memberships WHERE organization_id = ? AND user_id = ?', organization.organizationId, invitee.userId)
    ).toBe(1);
  });

  it('restricts invitation management to owners and admins', async () => {
    const owner = await signedInBrowser(upstream, 'manage-owner@example.com');
    const organization = await createOrganization(owner.browser);
    const member = await addMember(upstream, owner.browser, organization, 'manage-member@example.com');
    expect(await member.browser.json(`/api/organizations/${organization.organizationId}/invitations`)).toEqual({
      status: 403,
      body: { reasonCode: 'insufficient_permission' }
    });
  });
});
