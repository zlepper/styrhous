import type { InvitationRole } from '../db/schema';

export type InvitationEmail = Readonly<{
  kind: 'created' | 'resent';
  to: string;
  organizationName: string;
  role: InvitationRole;
  secret: string;
  expiresAt: string;
}>;

/**
 * Sends an invitation through the Email Service binding. Returns the failure reason instead
 * of throwing, because the invitation itself is already stored and can be resent.
 * Recipients, secrets, and message bodies are never logged.
 */
export async function sendInvitationEmail(
  env: Env,
  invitationId: string,
  email: InvitationEmail
): Promise<string | null> {
  const url = new URL('/invitations/accept', env.PUBLIC_ORIGIN);
  url.searchParams.set('secret', email.secret);
  const subject =
    email.kind === 'created'
      ? `You have been invited to join ${email.organizationName} on Styrhous`
      : `Your updated invitation to join ${email.organizationName} on Styrhous`;
  const expires = new Date(email.expiresAt).toUTCString();
  const role = email.role === 'admin' ? 'an admin' : 'a member';
  const text = [
    `You have been invited to join ${email.organizationName} on Styrhous as ${role}.`,
    '',
    `Accept the invitation: ${url}`,
    '',
    `This invitation expires ${expires}. Sign in with this email address to accept it.`
  ].join('\n');
  const html = `<p>You have been invited to join <strong>${escapeHtml(email.organizationName)}</strong> on Styrhous as ${role}.</p>
<p><a href="${escapeHtml(url.toString())}">Accept the invitation</a></p>
<p>This invitation expires ${escapeHtml(expires)}. Sign in with this email address to accept it.</p>`;

  try {
    await env.EMAIL.send({
      from: { name: 'Styrhous', email: env.INVITATION_FROM_ADDRESS },
      to: email.to,
      subject,
      text,
      html,
      headers: { 'X-Styrhous-Invitation-Id': invitationId }
    });
    return null;
  } catch (error) {
    const reason = error instanceof Error ? error.name : 'unknown_error';
    console.error('Invitation email delivery failed', { invitationId, reason });
    return reason;
  }
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}
