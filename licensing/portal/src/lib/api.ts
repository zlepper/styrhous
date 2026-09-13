export type OrganizationRole = 'owner' | 'admin' | 'member';

export type OrganizationSummary = Readonly<{
  organizationId: string;
  billingAccountId: string;
  membershipId: string;
  seatId: string;
  name: string;
  role: OrganizationRole;
  productSeatAssigned: boolean;
  deviceLimit: number;
  joinedAt: string;
}>;

export type OrganizationMember = Readonly<{
  membershipId: string;
  userId: string;
  seatId: string;
  email: string;
  role: OrganizationRole;
  productSeatAssigned: boolean;
  deviceLimit: number;
  joinedAt: string;
}>;

export type OrganizationInvitation = Readonly<{
  invitationId: string;
  createdByUserId: string;
  email: string;
  role: Exclude<OrganizationRole, 'owner'>;
  assignProductSeat: boolean;
  createdAt: string;
  lastSentAt: string;
  expiresAt: string;
}>;

export type OrganizationCreation = Readonly<{
  userId: string;
  organizationId: string;
  billingAccountId: string;
  ownerMembershipId: string;
  seatId: string;
  correlationId: string;
  trialWasTransferred: boolean;
}>;

export type OrganizationInvitationAcceptance = Readonly<{
  reasonCode: 'invitation_accepted';
  invitationId: string;
  organizationId: string;
  membershipId: string;
  seatId: string;
  productSeatAssigned: boolean;
  role: Exclude<OrganizationRole, 'owner'>;
  correlationId: string;
  acceptedAt: string;
}>;

export type OrganizationSeatAssignment = Readonly<{
  reasonCode: string;
  organizationId: string;
  membershipId: string;
  userId: string;
  seatId: string;
  assigned: boolean;
  deviceLimit: number;
  correlationId: string;
  changedAt: string;
}>;

export type SeatEntitlement = Readonly<{
  seatId: string;
  billingAccountId: string;
  state: 'evaluation' | 'trial' | 'commercial' | 'grace';
  reasonCode: string;
  isEligible: boolean;
  validFrom: string | null;
  validUntil: string | null;
}>;

export type ActiveDevice = Readonly<{
  activationId: string;
  installationId: string;
  displayName: string;
  platform: string;
  architecture: string;
  styrhousVersion: string;
  activatedAt: string;
  lastSeenAt: string;
}>;

export type DeviceList = Readonly<{
  reasonCode: string;
  seatId: string;
  deviceLimit: number;
  activeDevices: readonly ActiveDevice[];
}>;

export type DeviceAuthorizationInstallation = Readonly<{
  installationId: string;
  displayName: string;
  platform: string;
  architecture: string;
  styrhousVersion: string;
}>;

export type DeviceAuthorizationSeat = Readonly<{
  seatId: string;
  billingAccountId: string;
  name: string;
  entitlementState: SeatEntitlement['state'];
  entitlementReasonCode: string;
  deviceLimit: number;
  canActivate: boolean;
  activeDevices: readonly ActiveDevice[];
}>;

export type DeviceAuthorizationApproval = Readonly<{
  reasonCode: 'device_authorization_awaiting_approval';
  installation: DeviceAuthorizationInstallation;
  eligibleSeats: readonly DeviceAuthorizationSeat[];
  selectedSeatId: string | null;
}>;

export type BillingTrial = Readonly<{
  trialId: string;
  startedAt: string;
  endsAt: string;
  terminatedAt: string | null;
  transferredAt: string | null;
  isActive: boolean;
}>;

export type BillingSubscription = Readonly<{
  subscriptionId: string;
  status:
    | 'active'
    | 'past_due'
    | 'unpaid'
    | 'paused'
    | 'incomplete'
    | 'incomplete_expired'
    | 'trialing'
    | 'canceled';
  seatQuantity: number;
  cancelAtPeriodEnd: boolean;
  currentPeriodStartedAt: string;
  currentPeriodEndsAt: string;
  projectedAt: string;
}>;

export type BillingAccount = Readonly<{
  billingAccountId: string;
  accountKind: 'personal' | 'organization';
  organizationId: string | null;
  organizationName: string | null;
  organizationRole: OrganizationRole | null;
  canManageBilling: boolean;
  canStartCheckout: boolean;
  assignedSeatCount: number;
  entitlement: SeatEntitlement;
  trial: BillingTrial | null;
  subscription: BillingSubscription | null;
}>;

export type BillingCadence = 'monthly' | 'annual';

export type BillingPrice = Readonly<{
  cadence: BillingCadence;
  unitAmount: number;
  currency: string;
  taxIncluded: boolean | null;
}>;

const authenticationProviders = ['github', 'google', 'microsoft'] as const;

export type AuthenticationProvider = (typeof authenticationProviders)[number];

const authenticationProviderLabels: Readonly<Record<AuthenticationProvider, string>> = {
  github: 'GitHub',
  google: 'Google',
  microsoft: 'Microsoft'
};

const authenticationFailureMessages: Readonly<Record<string, string>> = {
  microsoft_verified_email_required: 'Microsoft did not supply a verified email address. Please sign in with another provider.',
  verified_email_required: 'Your identity provider did not supply a verified email address.',
  authentication_provider_changed: 'The sign-in provider changed before authentication completed.',
  invalid_external_identity: 'The identity provider returned an invalid account identity.',
  authentication_operation_invalid: 'That authentication attempt is no longer valid.',
  authentication_session_changed: 'Your browser session changed during authentication. Please try again.',
  provider_not_linked: 'That sign-in provider is not linked to this Styrhous account.',
  account_link_required:
    'That sign-in belongs to another Styrhous account. Sign in first, then link the provider from Account.',
  verified_email_mismatch: 'The provider email does not match this Styrhous account.',
  provider_already_linked: 'That provider is already linked to a Styrhous account.',
  concurrent_modification: 'The account changed during authentication. Review it and try again.',
  provider_authentication_failed: 'The identity provider could not complete authentication.'
};

export function authenticationProviderLabel(provider: AuthenticationProvider): string {
  return authenticationProviderLabels[provider];
}

export function authenticationFailureMessage(reasonCode: string | null): string | null {
  if (!reasonCode) return null;
  return (
    authenticationFailureMessages[reasonCode] ??
    'Authentication could not be completed. Please try again.'
  );
}

export type AuthenticationSession = Readonly<{
  authenticated: boolean;
  userId?: string;
  email?: string;
  linkedProviders?: readonly AuthenticationProvider[];
  configuredProviders: readonly AuthenticationProvider[];
  recentlyAuthenticated?: boolean;
}>;

export type BillingCheckoutSession = Readonly<{
  reasonCode: 'checkout_session_created';
  billingOperationId: string;
  redirectUrl: string;
}>;

export type BillingCustomerPortalSession = Readonly<{
  reasonCode: 'customer_portal_session_created';
  redirectUrl: string;
}>;

export type BillingSeatQuantityChange = Readonly<{
  reasonCode: 'seat_quantity_changed';
  billingOperationId: string;
  seatQuantity: number;
}>;

type OrganizationListResponse = Readonly<{
  reasonCode: string;
  organizations: readonly OrganizationSummary[];
}>;

type OrganizationMemberListResponse = Readonly<{
  reasonCode: string;
  organizationId: string;
  members: readonly OrganizationMember[];
}>;

type OrganizationInvitationListResponse = Readonly<{
  reasonCode: string;
  organizationId: string;
  invitations: readonly OrganizationInvitation[];
}>;

type EntitlementListResponse = Readonly<{
  entitlements: readonly SeatEntitlement[];
}>;

type BillingAccountListResponse = Readonly<{
  reasonCode: string;
  accounts: readonly BillingAccount[];
}>;

type AntiforgeryTokenResponse = Readonly<{
  requestToken: string;
}>;

type ApiErrorPayload = Readonly<{
  reasonCode?: unknown;
  title?: unknown;
  errors?: unknown;
  billingOperationId?: unknown;
  requiredSeatQuantity?: unknown;
  cadence?: unknown;
  seatQuantity?: unknown;
  previousSeatQuantity?: unknown;
  expiresAt?: unknown;
}>;

export class PortalAuthenticationRequiredError extends Error {
  constructor() {
    super('Your Styrhous session is required to view this page.');
    this.name = 'PortalAuthenticationRequiredError';
  }
}

export class PortalApiError extends Error {
  readonly status: number;
  readonly reasonCode: string | null;
  readonly validationErrors: Readonly<Record<string, readonly string[]>>;
  readonly details: ApiErrorPayload;

  constructor(
    status: number,
    message: string,
    reasonCode: string | null,
    validationErrors: Readonly<Record<string, readonly string[]>>,
    details: ApiErrorPayload = {}
  ) {
    super(message);
    this.name = 'PortalApiError';
    this.status = status;
    this.reasonCode = reasonCode;
    this.validationErrors = validationErrors;
    this.details = details;
  }
}

export type PortalApi = ReturnType<typeof createPortalApi>;

export function createPortalApi(fetcher: typeof fetch = globalThis.fetch) {
  async function send(path: string, init?: RequestInit, mutating = false): Promise<Response> {
    const headers = new Headers(init?.headers);
    headers.set('Accept', 'application/json');
    if (mutating) {
      headers.set('X-CSRF-TOKEN', await getAntiforgeryToken());
    }

    const response = await fetcher(path, {
      ...init,
      credentials: 'same-origin',
      headers
    });
    if (response.status === 401) {
      throw new PortalAuthenticationRequiredError();
    }

    if (!response.ok) {
      throw await toApiError(response);
    }

    return response;
  }

  async function request<T>(path: string, init?: RequestInit, mutating = false): Promise<T> {
    const response = await send(path, init, mutating);
    return (await response.json()) as T;
  }

  async function getAntiforgeryToken(): Promise<string> {
    return (await request<AntiforgeryTokenResponse>('/api/antiforgery')).requestToken;
  }

  function mutation<T>(
    path: string,
    method: 'POST' | 'PATCH' | 'DELETE',
    body?: unknown,
    signal?: AbortSignal
  ) {
    const headers = new Headers();
    let serializedBody: string | undefined;
    if (body !== undefined) {
      headers.set('Content-Type', 'application/json');
      serializedBody = JSON.stringify(body);
    }

    return request<T>(
      path,
      {
        method,
        headers,
        body: serializedBody,
        signal
      },
      true
    );
  }

  async function formMutation(
    path: string,
    fields: Readonly<Record<string, string>>,
    signal?: AbortSignal
  ): Promise<void> {
    await send(
      path,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams(fields).toString(),
        signal
      },
      true
    );
  }

  return {
    async listAuthenticationProviders(): Promise<readonly AuthenticationProvider[]> {
      const response = await request<{ providers?: unknown }>('/auth/providers');
      if (!isAuthenticationProviderList(response.providers)) {
        throw new Error('The licensing service returned invalid authentication providers.');
      }
      return response.providers;
    },

    async getAuthenticationSession(): Promise<AuthenticationSession> {
      return parseAuthenticationSession(await request<unknown>('/auth/session'));
    },

    unlinkAuthenticationProvider(provider: AuthenticationProvider): Promise<void> {
      return mutation(`/auth/providers/${encodeURIComponent(provider)}`, 'DELETE');
    },

    async signOut(): Promise<void> {
      await send('/auth/sign-out', { method: 'POST' }, true);
    },

    async listOrganizations(): Promise<readonly OrganizationSummary[]> {
      return (await request<OrganizationListResponse>('/api/organizations')).organizations;
    },

    createOrganization(name: string): Promise<OrganizationCreation> {
      return mutation<OrganizationCreation>('/api/organizations', 'POST', { name });
    },

    async listMembers(organizationId: string): Promise<readonly OrganizationMember[]> {
      return (
        await request<OrganizationMemberListResponse>(
          `/api/organizations/${encodeURIComponent(organizationId)}/members`
        )
      ).members;
    },

    removeMember(organizationId: string, membershipId: string) {
      return mutation(
        `/api/organizations/${encodeURIComponent(organizationId)}/members/${encodeURIComponent(membershipId)}`,
        'DELETE'
      );
    },

    changeMemberRole(
      organizationId: string,
      membershipId: string,
      role: Exclude<OrganizationRole, 'owner'>
    ) {
      return mutation(
        `/api/organizations/${encodeURIComponent(organizationId)}/members/${encodeURIComponent(membershipId)}/role`,
        'PATCH',
        { role }
      );
    },

    changeMemberSeat(
      organizationId: string,
      membershipId: string,
      assigned: boolean
    ): Promise<OrganizationSeatAssignment> {
      return mutation(
        `/api/organizations/${encodeURIComponent(organizationId)}/members/${encodeURIComponent(membershipId)}/seat`,
        'PATCH',
        { assigned }
      );
    },

    transferOwnership(organizationId: string, membershipId: string) {
      return mutation(
        `/api/organizations/${encodeURIComponent(organizationId)}/members/${encodeURIComponent(membershipId)}/transfer-ownership`,
        'POST'
      );
    },

    async listInvitations(organizationId: string): Promise<readonly OrganizationInvitation[]> {
      return (
        await request<OrganizationInvitationListResponse>(
          `/api/organizations/${encodeURIComponent(organizationId)}/invitations`
        )
      ).invitations;
    },

    createInvitation(
      organizationId: string,
      email: string,
      role: Exclude<OrganizationRole, 'owner'>,
      assignProductSeat: boolean
    ) {
      return mutation(
        `/api/organizations/${encodeURIComponent(organizationId)}/invitations`,
        'POST',
        { email, role, assignProductSeat }
      );
    },

    acceptInvitation(secret: string): Promise<OrganizationInvitationAcceptance> {
      return mutation<OrganizationInvitationAcceptance>(
        '/api/invitations/accept',
        'POST',
        { secret }
      );
    },

    cancelInvitation(organizationId: string, invitationId: string) {
      return mutation(
        `/api/organizations/${encodeURIComponent(organizationId)}/invitations/${encodeURIComponent(invitationId)}`,
        'DELETE'
      );
    },

    resendInvitation(organizationId: string, invitationId: string) {
      return mutation(
        `/api/organizations/${encodeURIComponent(organizationId)}/invitations/${encodeURIComponent(invitationId)}/resend`,
        'POST'
      );
    },

    async listEntitlements(): Promise<readonly SeatEntitlement[]> {
      return (await request<EntitlementListResponse>('/api/entitlements')).entitlements;
    },

    async listBillingAccounts(): Promise<readonly BillingAccount[]> {
      return (await request<BillingAccountListResponse>('/api/billing-accounts')).accounts;
    },

    async listBillingPrices(): Promise<readonly BillingPrice[]> {
      return (await request<{ prices: readonly BillingPrice[] }>('/api/billing-prices')).prices;
    },

    createCheckoutSession(
      billingAccountId: string,
      cadence: BillingCadence,
      seatQuantity: number,
      billingOperationId: string | null = null
    ): Promise<BillingCheckoutSession> {
      return mutation<BillingCheckoutSession>(
        `/api/billing-accounts/${encodeURIComponent(billingAccountId)}/checkout-sessions`,
        'POST',
        { cadence, seatQuantity, billingOperationId }
      );
    },

    createCustomerPortalSession(
      billingAccountId: string,
      signal?: AbortSignal
    ): Promise<BillingCustomerPortalSession> {
      return mutation<BillingCustomerPortalSession>(
        `/api/billing-accounts/${encodeURIComponent(billingAccountId)}/customer-portal-sessions`,
        'POST',
        undefined,
        signal
      );
    },

    changeSeatQuantity(
      billingAccountId: string,
      seatQuantity: number,
      billingOperationId: string | null = null,
      signal?: AbortSignal
    ): Promise<BillingSeatQuantityChange> {
      return mutation<BillingSeatQuantityChange>(
        `/api/billing-accounts/${encodeURIComponent(billingAccountId)}/seat-quantity`,
        'PATCH',
        { seatQuantity, billingOperationId },
        signal
      );
    },

    listDevices(seatId: string): Promise<DeviceList> {
      return request<DeviceList>(`/api/seats/${encodeURIComponent(seatId)}/devices`);
    },

    revokeDevice(activationId: string) {
      return mutation(`/api/device-activations/${encodeURIComponent(activationId)}`, 'DELETE');
    },

    getDeviceAuthorizationApproval(
      userCode: string,
      signal?: AbortSignal
    ): Promise<DeviceAuthorizationApproval> {
      return request<DeviceAuthorizationApproval>(
        `/desktop/v1/device/approval?user_code=${encodeURIComponent(userCode)}`,
        { signal }
      );
    },

    approveDeviceAuthorization(
      userCode: string,
      seatId: string,
      signal?: AbortSignal
    ): Promise<void> {
      return formMutation(
        '/desktop/v1/device/approval',
        { user_code: userCode, decision: 'approve', seat_id: seatId },
        signal
      );
    },

    denyDeviceAuthorization(userCode: string, signal?: AbortSignal): Promise<void> {
      return formMutation(
        '/desktop/v1/device/approval',
        { user_code: userCode, decision: 'deny' },
        signal
      );
    }
  };
}

export function portalErrorMessage(error: unknown): string {
  if (error instanceof PortalAuthenticationRequiredError) {
    return error.message;
  }

  if (error instanceof PortalApiError) {
    const validationMessage = Object.values(error.validationErrors).flat()[0];
    if (validationMessage) {
      return validationMessage;
    }

    if (error.reasonCode) {
      return reasonCodeMessage(error);
    }

    return error.message;
  }

  return 'The licensing service could not complete the request. Please try again.';
}

async function toApiError(response: Response): Promise<PortalApiError> {
  let payload: ApiErrorPayload = {};
  try {
    payload = (await response.json()) as ApiErrorPayload;
  } catch {
    // A non-JSON proxy or hosting error still gets a stable portal message.
  }

  const reasonCode = typeof payload.reasonCode === 'string' ? payload.reasonCode : null;
  const validationErrors = parseValidationErrors(payload.errors);
  const title = typeof payload.title === 'string' ? payload.title : null;
  return new PortalApiError(
    response.status,
    title ?? `The licensing service returned HTTP ${response.status}.`,
    reasonCode,
    validationErrors,
    payload
  );
}

export type BillingCheckoutErrorDetails = Readonly<{
  billingOperationId: string | null;
  requiredSeatQuantity: number | null;
  cadence: BillingCadence | null;
  seatQuantity: number | null;
  expiresAt: string | null;
}>;

export type BillingSeatQuantityErrorDetails = Readonly<{
  billingOperationId: string | null;
  previousSeatQuantity: number | null;
  seatQuantity: number | null;
  requiredSeatQuantity: number | null;
}>;

export function billingSeatQuantityErrorDetails(
  error: PortalApiError
): BillingSeatQuantityErrorDetails {
  const details = error.details;
  return {
    billingOperationId:
      typeof details.billingOperationId === 'string' ? details.billingOperationId : null,
    previousSeatQuantity: positiveSafeInteger(details.previousSeatQuantity),
    seatQuantity: positiveSafeInteger(details.seatQuantity),
    requiredSeatQuantity: positiveSafeInteger(details.requiredSeatQuantity)
  };
}

export function billingCheckoutErrorDetails(error: PortalApiError): BillingCheckoutErrorDetails {
  const details = error.details;
  return {
    billingOperationId:
      typeof details.billingOperationId === 'string' ? details.billingOperationId : null,
    requiredSeatQuantity: positiveSafeInteger(details.requiredSeatQuantity),
    cadence: details.cadence === 'monthly' || details.cadence === 'annual'
      ? details.cadence
      : null,
    seatQuantity: positiveSafeInteger(details.seatQuantity),
    expiresAt: typeof details.expiresAt === 'string' ? details.expiresAt : null
  };
}

function positiveSafeInteger(value: unknown): number | null {
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0
    ? value
    : null;
}

function isAuthenticationProvider(value: unknown): value is AuthenticationProvider {
  return (
    typeof value === 'string' &&
    (authenticationProviders as readonly string[]).includes(value)
  );
}

function parseAuthenticationSession(value: unknown): AuthenticationSession {
  if (!value || typeof value !== 'object') {
    throw invalidAuthenticationSession();
  }

  const session = value as Readonly<Record<string, unknown>>;
  if (
    typeof session.authenticated !== 'boolean' ||
    !isAuthenticationProviderList(session.configuredProviders)
  ) {
    throw invalidAuthenticationSession();
  }

  if (!session.authenticated) {
    return {
      authenticated: false,
      configuredProviders: session.configuredProviders
    };
  }

  if (
    typeof session.userId !== 'string' ||
    typeof session.email !== 'string' ||
    !isAuthenticationProviderList(session.linkedProviders) ||
    session.linkedProviders.length === 0 ||
    typeof session.recentlyAuthenticated !== 'boolean'
  ) {
    throw invalidAuthenticationSession();
  }

  return {
    authenticated: true,
    userId: session.userId,
    email: session.email,
    linkedProviders: session.linkedProviders,
    configuredProviders: session.configuredProviders,
    recentlyAuthenticated: session.recentlyAuthenticated
  };
}

function isAuthenticationProviderList(value: unknown): value is readonly AuthenticationProvider[] {
  return Array.isArray(value) && value.every(isAuthenticationProvider);
}

function invalidAuthenticationSession(): Error {
  return new Error('The licensing service returned an invalid authentication session.');
}

function parseValidationErrors(value: unknown): Readonly<Record<string, readonly string[]>> {
  if (!value || typeof value !== 'object') {
    return {};
  }

  return Object.fromEntries(
    Object.entries(value)
      .filter((entry): entry is [string, string[]] =>
        Array.isArray(entry[1]) && entry[1].every((item) => typeof item === 'string')
      )
      .map(([key, messages]) => [key, messages])
  );
}

function reasonCodeMessage(error: PortalApiError): string {
  const checkoutDetails = billingCheckoutErrorDetails(error);
  const messages: Readonly<Record<string, string>> = {
    organization_not_found: 'That organization is no longer available.',
    organization_member_not_found: 'That organization member is no longer available.',
    invitation_not_found: 'That invitation is no longer available.',
    invitation_email_mismatch: 'Sign in with the email address that received this invitation.',
    insufficient_permission: 'Your organization role does not allow this action.',
    ownership_transfer_required: 'Transfer ownership before removing the owner.',
    ownership_transfer_target_invalid: 'Choose another organization member as the new owner.',
    organization_member_role_unchanged: 'That member already has the selected role.',
    organization_member_seat_unchanged: 'That member already has the selected product seat access.',
    organization_members_changed: 'The roster changed while you were editing it. Review it and try again.',
    already_member: 'That person is already an organization member.',
    invitation_already_pending: 'An active invitation already exists for that email address.',
    no_active_seat_capacity: 'This organization does not currently have active seat capacity.',
    seat_capacity_reached: 'All available organization seats are assigned or reserved.',
    personal_seat_quantity_invalid: 'Personal plans always contain exactly one seat.',
    seat_quantity_too_small:
      checkoutDetails.requiredSeatQuantity
        ? `Choose at least ${checkoutDetails.requiredSeatQuantity} seats to cover product seats in use and seat-bearing invitations.`
        : 'Choose enough seats to cover product seats in use and seat-bearing invitations.',
    subscription_already_exists: 'This account already has a subscription to manage.',
    subscription_not_found: 'This account does not have a subscription to manage.',
    billing_operation_not_found: 'That billing attempt can no longer be retried.',
    checkout_operation_in_progress:
      'A recent checkout is still active. Continue that attempt before changing its cadence or seats.',
    checkout_operation_capacity_changed:
      'The active Checkout no longer covers product seats in use and seat-bearing invitations. Wait for it to expire before starting a replacement.',
    checkout_provider_unavailable: 'Checkout is temporarily unavailable. Try again.',
    customer_portal_provider_unavailable:
      'Billing management is temporarily unavailable. Try again.',
    seat_quantity_unchanged: 'The subscription already has that seat quantity.',
    subscription_inactive:
      'This subscription is not active enough to change its seat quantity.',
    seat_quantity_operation_in_progress:
      'A seat change is already pending. Continue that exact change before starting another.',
    subscription_quantity_changed:
      'The seat count changed. Review the updated subscription before trying again.',
    seat_quantity_provider_unavailable:
      'The seat change could not be completed. Retry the same change.',
    seat_quantity_reconciliation_required:
      'This seat change may already have completed. Check again later or contact support; another update will not be submitted automatically.',
    invitation_cancellation_superseded: 'The invitation changed before it could be cancelled.',
    invitation_resend_superseded: 'A newer invitation email has already been sent.',
    seat_not_found: 'That seat is no longer available.',
    device_not_active: 'That desktop device is no longer active.',
    device_authorization_not_found:
      'That desktop authorization is invalid or has expired. Start again from Styrhous.',
    device_authorization_invalid_request:
      'That desktop approval request is invalid. Reload it and try again.',
    device_authorization_seat_required: 'Choose the seat that should own this desktop device.',
    device_authorization_seat_not_eligible:
      'That seat is no longer eligible. Choose another seat or review your licensing status.',
    device_authorization_concurrent_modification:
      'That desktop authorization changed while you were deciding. Review it and try again.',
    device_limit_reached:
      'This seat has no stale device to replace. Revoke an active device before approving this one.',
    recent_authentication_required: 'Sign in again to change your sign-in methods.',
    provider_not_linked: 'That sign-in provider is no longer linked to this account.',
    last_provider_required: 'Keep at least one sign-in method connected to this account.',
    concurrent_modification: 'The account changed while you were editing it. Review it and try again.'
  };
  return messages[error.reasonCode ?? '']
    ?? 'The licensing service rejected this request. Review and try again.';
}
