import { sql } from 'drizzle-orm';
import {
  check,
  customType,
  index,
  integer,
  sqliteTable,
  text,
  uniqueIndex
} from 'drizzle-orm/sqlite-core';

// Identifiers are UUIDv7 strings and timestamps are ISO-8601 UTC strings, which sort
// chronologically as text.

/** Email addresses compare case-insensitively (ASCII) in indexes and `=` predicates. */
const emailText = customType<{ data: string }>({
  dataType: () => 'text COLLATE NOCASE'
});

export const memberRoles = ['owner', 'admin', 'member'] as const;
export type MemberRole = (typeof memberRoles)[number];

/** Owners are made by creating an organization or transferring ownership, never by invitation. */
export const invitationRoles = ['admin', 'member'] as const;
export type InvitationRole = (typeof invitationRoles)[number];

export const users = sqliteTable('users', {
  id: text('id').primaryKey(),
  email: text('email').notNull(),
  createdAt: text('created_at').notNull()
});

export const externalIdentities = sqliteTable(
  'external_identities',
  {
    id: text('id').primaryKey(),
    userId: text('user_id').notNull().references(() => users.id),
    provider: text('provider').notNull(),
    subject: text('subject').notNull(),
    createdAt: text('created_at').notNull()
  },
  (table) => [
    uniqueIndex('external_identities_provider_subject').on(table.provider, table.subject),
    uniqueIndex('external_identities_user_provider').on(table.userId, table.provider)
  ]
);

/** One account per verified email address: the unique index is the source of truth. */
export const verifiedEmails = sqliteTable(
  'verified_emails',
  {
    id: text('id').primaryKey(),
    userId: text('user_id').notNull().references(() => users.id),
    email: emailText('email').notNull(),
    createdAt: text('created_at').notNull()
  },
  (table) => [uniqueIndex('verified_emails_email').on(table.email)]
);

export const sessions = sqliteTable(
  'sessions',
  {
    id: text('id').primaryKey(),
    tokenHash: text('token_hash').notNull(),
    userId: text('user_id').notNull().references(() => users.id),
    provider: text('provider').notNull(),
    authTime: text('auth_time').notNull(),
    createdAt: text('created_at').notNull(),
    expiresAt: text('expires_at').notNull()
  },
  (table) => [
    uniqueIndex('sessions_token_hash').on(table.tokenHash),
    index('sessions_user').on(table.userId)
  ]
);

export const oauthOperations = ['sign-in', 'link', 'reauth'] as const;
export type OAuthOperation = (typeof oauthOperations)[number];

export const oauthStates = sqliteTable('oauth_states', {
  stateHash: text('state_hash').primaryKey(),
  codeVerifier: text('code_verifier').notNull(),
  provider: text('provider').notNull(),
  operation: text('operation', { enum: oauthOperations }).notNull(),
  returnUrl: text('return_url').notNull(),
  sessionId: text('session_id'),
  expiresAt: text('expires_at').notNull()
});

export const billingAccounts = sqliteTable(
  'billing_accounts',
  {
    id: text('id').primaryKey(),
    kind: text('kind', { enum: ['personal', 'organization'] }).notNull(),
    personalOwnerUserId: text('personal_owner_user_id').references(() => users.id),
    stripeCustomerId: text('stripe_customer_id'),
    openCheckoutSessionId: text('open_checkout_session_id'),
    openCheckoutExpiresAt: text('open_checkout_expires_at'),
    createdAt: text('created_at').notNull()
  },
  (table) => [
    uniqueIndex('billing_accounts_personal_owner')
      .on(table.personalOwnerUserId)
      .where(sql`kind = 'personal'`),
    check(
      'billing_accounts_personal_owner_matches_kind',
      sql`(kind = 'personal') = (personal_owner_user_id IS NOT NULL)`
    )
  ]
);

export const seats = sqliteTable(
  'seats',
  {
    id: text('id').primaryKey(),
    billingAccountId: text('billing_account_id').notNull().references(() => billingAccounts.id),
    userId: text('user_id').notNull().references(() => users.id),
    deviceLimit: integer('device_limit').notNull().default(3),
    productAccess: integer('product_access', { mode: 'boolean' }).notNull(),
    createdAt: text('created_at').notNull()
  },
  (table) => [
    uniqueIndex('seats_billing_account_user').on(table.billingAccountId, table.userId),
    index('seats_user').on(table.userId),
    check('seats_device_limit_positive', sql`device_limit > 0`)
  ]
);

export const trials = sqliteTable(
  'trials',
  {
    id: text('id').primaryKey(),
    originatingUserId: text('originating_user_id').notNull().references(() => users.id),
    billingAccountId: text('billing_account_id').notNull().references(() => billingAccounts.id),
    startedAt: text('started_at').notNull(),
    endsAt: text('ends_at').notNull(),
    transferredAt: text('transferred_at'),
    terminatedAt: text('terminated_at')
  },
  (table) => [
    uniqueIndex('trials_originating_user').on(table.originatingUserId),
    uniqueIndex('trials_billing_account').on(table.billingAccountId),
    check('trials_ends_after_start', sql`ends_at > started_at`)
  ]
);

export const organizations = sqliteTable(
  'organizations',
  {
    id: text('id').primaryKey(),
    billingAccountId: text('billing_account_id').notNull().references(() => billingAccounts.id),
    createdByUserId: text('created_by_user_id').notNull().references(() => users.id),
    name: text('name').notNull(),
    createdAt: text('created_at').notNull()
  },
  (table) => [uniqueIndex('organizations_billing_account').on(table.billingAccountId)]
);

export const memberships = sqliteTable(
  'memberships',
  {
    id: text('id').primaryKey(),
    organizationId: text('organization_id').notNull().references(() => organizations.id),
    userId: text('user_id').notNull().references(() => users.id),
    role: text('role', { enum: memberRoles }).notNull(),
    createdAt: text('created_at').notNull()
  },
  (table) => [
    uniqueIndex('memberships_organization_user').on(table.organizationId, table.userId),
    uniqueIndex('memberships_single_owner')
      .on(table.organizationId)
      .where(sql`role = 'owner'`),
    index('memberships_user').on(table.userId)
  ]
);

export const invitations = sqliteTable(
  'invitations',
  {
    id: text('id').primaryKey(),
    organizationId: text('organization_id').notNull().references(() => organizations.id),
    createdByUserId: text('created_by_user_id').notNull().references(() => users.id),
    email: emailText('email').notNull(),
    role: text('role', { enum: invitationRoles }).notNull(),
    assignProductSeat: integer('assign_product_seat', { mode: 'boolean' }).notNull(),
    secretHash: text('secret_hash').notNull(),
    createdAt: text('created_at').notNull(),
    lastSentAt: text('last_sent_at').notNull(),
    expiresAt: text('expires_at').notNull(),
    acceptedAt: text('accepted_at'),
    acceptedByUserId: text('accepted_by_user_id').references(() => users.id),
    cancelledAt: text('cancelled_at'),
    lastDeliveryError: text('last_delivery_error')
  },
  (table) => [
    uniqueIndex('invitations_secret_hash').on(table.secretHash),
    index('invitations_pending')
      .on(table.organizationId, table.email, table.expiresAt)
      .where(sql`accepted_at IS NULL AND cancelled_at IS NULL`),
    check(
      'invitations_single_outcome',
      sql`accepted_at IS NULL OR cancelled_at IS NULL`
    )
  ]
);

export const subscriptionStatuses = [
  'active',
  'past_due',
  'unpaid',
  'paused',
  'incomplete',
  'incomplete_expired',
  'trialing',
  'canceled'
] as const;

export type SubscriptionStatus = (typeof subscriptionStatuses)[number];

/** Statuses that keep a subscription's paid entitlement. */
export const liveStatuses: readonly SubscriptionStatus[] = ['active', 'past_due'];

/** Statuses after which a billing account may purchase a new subscription. */
export const finishedStatuses: readonly SubscriptionStatus[] = ['canceled', 'incomplete_expired'];

export const subscriptions = sqliteTable(
  'subscriptions',
  {
    id: text('id').primaryKey(),
    billingAccountId: text('billing_account_id').notNull().references(() => billingAccounts.id),
    stripeCustomerId: text('stripe_customer_id').notNull(),
    stripeSubscriptionId: text('stripe_subscription_id').notNull(),
    stripePriceId: text('stripe_price_id').notNull(),
    status: text('status', { enum: subscriptionStatuses }).notNull(),
    seatQuantity: integer('seat_quantity').notNull(),
    cancelAtPeriodEnd: integer('cancel_at_period_end', { mode: 'boolean' }).notNull(),
    currentPeriodStart: text('current_period_start').notNull(),
    currentPeriodEnd: text('current_period_end').notNull(),
    syncedAt: text('synced_at').notNull()
  },
  (table) => [
    uniqueIndex('subscriptions_billing_account').on(table.billingAccountId),
    uniqueIndex('subscriptions_stripe_subscription').on(table.stripeSubscriptionId),
    check('subscriptions_seat_quantity_positive', sql`seat_quantity > 0`)
  ]
);

export const deviceActivations = sqliteTable(
  'device_activations',
  {
    id: text('id').primaryKey(),
    seatId: text('seat_id').notNull().references(() => seats.id),
    userId: text('user_id').notNull().references(() => users.id),
    installationId: text('installation_id').notNull(),
    displayName: text('display_name').notNull(),
    platform: text('platform').notNull(),
    architecture: text('architecture').notNull(),
    styrhousVersion: text('styrhous_version').notNull(),
    /** Hash of the desktop installation token; null until the device code is exchanged. */
    tokenHash: text('token_hash'),
    activatedAt: text('activated_at').notNull(),
    lastSeenAt: text('last_seen_at').notNull(),
    revokedAt: text('revoked_at'),
    revocationReason: text('revocation_reason')
  },
  (table) => [
    uniqueIndex('device_activations_active_installation')
      .on(table.seatId, table.installationId)
      .where(sql`revoked_at IS NULL`),
    uniqueIndex('device_activations_token_hash').on(table.tokenHash),
    index('device_activations_seat').on(table.seatId)
  ]
);

export const deviceAuthorizations = sqliteTable(
  'device_authorizations',
  {
    id: text('id').primaryKey(),
    deviceCodeHash: text('device_code_hash').notNull(),
    userCode: text('user_code').notNull(),
    installationId: text('installation_id').notNull(),
    displayName: text('display_name').notNull(),
    platform: text('platform').notNull(),
    architecture: text('architecture').notNull(),
    styrhousVersion: text('styrhous_version').notNull(),
    status: text('status', { enum: ['pending', 'approved', 'denied', 'consumed'] }).notNull(),
    activationId: text('activation_id').references(() => deviceActivations.id),
    createdAt: text('created_at').notNull(),
    expiresAt: text('expires_at').notNull(),
    lastPolledAt: text('last_polled_at')
  },
  (table) => [
    uniqueIndex('device_authorizations_device_code_hash').on(table.deviceCodeHash),
    uniqueIndex('device_authorizations_user_code').on(table.userCode)
  ]
);

export const auditRecords = sqliteTable(
  'audit_records',
  {
    id: text('id').primaryKey(),
    correlationId: text('correlation_id').notNull(),
    actorUserId: text('actor_user_id'),
    action: text('action').notNull(),
    targetType: text('target_type').notNull(),
    targetId: text('target_id').notNull(),
    occurredAt: text('occurred_at').notNull()
  },
  (table) => [index('audit_records_target').on(table.targetType, table.targetId)]
);
