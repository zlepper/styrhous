import { expect, vi } from 'vitest';
import { env } from 'cloudflare:workers';
import type { Browser } from './portal.ts';
import { FakeUpstream } from './upstream.ts';
import { signedInBrowser } from './identity.ts';

export type Organization = Readonly<{ organizationId: string; billingAccountId: string; ownerMembershipId: string }>;

export async function createOrganization(browser: Browser, name = 'Acme'): Promise<Organization> {
  const response = await browser.json<Organization>('/api/organizations', 'POST', { name });
  expect(response.status).toBe(201);
  return response.body;
}

/**
 * A signed-in user with a licensed seat: personal accounts start without a trial, so this creates
 * the user's first organization, which starts one.
 */
export async function licensedBrowser(upstream: FakeUpstream, email: string, organizationName = 'Acme') {
  const user = await signedInBrowser(upstream, email);
  const organization = await createOrganization(user.browser, organizationName);
  return { ...user, organization };
}

/** Captures invitation emails sent through the Email Service binding. */
export function captureEmails() {
  return vi.spyOn(env.EMAIL, 'send').mockResolvedValue({ messageId: 'test-message' });
}

export function secretFrom(email: { text?: string }): string {
  return new URL(/https:\/\/\S+/.exec(email.text ?? '')![0]).searchParams.get('secret')!;
}

/** Invites a new user into the organization and returns their signed-in browser and membership. */
export async function addMember(
  upstream: FakeUpstream,
  owner: Browser,
  organization: Organization,
  email: string,
  options: { role?: 'admin' | 'member'; assignProductSeat?: boolean } = {}
) {
  const sent = captureEmails();
  const invitation = await owner.json(`/api/organizations/${organization.organizationId}/invitations`, 'POST', {
    email,
    role: options.role ?? 'member',
    assignProductSeat: options.assignProductSeat ?? true
  });
  expect(invitation.status).toBe(201);
  const message = sent.mock.calls.at(-1)![0] as { text?: string };
  sent.mockRestore();
  const member = await signedInBrowser(upstream, email);
  const accepted = await member.browser.json<{ membershipId: string; seatId: string }>(
    '/api/invitations/accept',
    'POST',
    { secret: secretFrom(message) }
  );
  expect(accepted.status).toBe(200);
  return { ...member, membershipId: accepted.body.membershipId, seatId: accepted.body.seatId };
}
