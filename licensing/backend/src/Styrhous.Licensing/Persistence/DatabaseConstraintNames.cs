namespace Styrhous.Licensing.Persistence;

internal static class DatabaseConstraintNames
{
    public const string ExternalIdentityProviderSubject =
        "ux_external_identities_provider_subject";

    public const string VerifiedEmailClaimNormalizedEmail =
        "ux_verified_email_claims_normalized_email";

    public const string PersonalBillingAccountOwner =
        "ux_billing_accounts_personal_owner_user_id";

    public const string TrialBillingAccount = "ux_trials_billing_account_id";

    public const string TrialOriginatingUser = "ux_trials_originating_user_id";

    public const string TrialValidWindow = "ck_trials_valid_window";

    public const string TrialTermination = "ck_trials_termination";

    public const string SeatDeviceLimit = "ck_seats_device_limit";

    public const string ActiveDeviceInstallation =
        "ux_device_activations_active_seat_installation";

    public const string DeviceActivationTimestamps =
        "ck_device_activations_timestamps";

    public const string OrganizationInvitationSecretHash =
        "ux_organization_invitations_secret_hash";

    public const string OrganizationInvitationValidWindow =
        "ck_organization_invitations_valid_window";

    public const string OrganizationInvitationTerminalState =
        "ck_organization_invitations_terminal_state";

    public const string OrganizationInvitationReservedSeatCapacity =
        "ck_organization_invitations_reserved_seat_capacity";

    public const string OutboxLifecycle = "ck_outbox_messages_lifecycle";

    public const string OutboxProcessingLease =
        "ck_outbox_messages_processing_lease";

    public const string OutboxProcessingAttempts =
        "ck_outbox_messages_processing_attempts";

    public const string OutboxProcessingState =
        "ck_outbox_messages_processing_state";

    public const string CommercialSubscriptionValidPeriod =
        "ck_commercial_subscriptions_valid_period";

    public const string CommercialSubscriptionSeatQuantity =
        "ck_commercial_subscriptions_seat_quantity";

    public const string CommercialSubscriptionStatus =
        "ck_commercial_subscriptions_status";

    public const string CommercialSubscriptionProviderReadRevision =
        "ck_commercial_subscriptions_provider_read_revision";

    public const string CommercialSubscriptionProviderSnapshotKind =
        "ck_commercial_subscriptions_provider_snapshot_kind";

    public const string BillingProviderReadCursorRevision =
        "ck_billing_provider_read_cursors_revision";

    public const string BillingOperationSeatQuantity =
        "ck_billing_operations_seat_quantity";

    public const string BillingOperationKind = "ck_billing_operations_kind";

    public const string BillingOperationCadence = "ck_billing_operations_cadence";

    public const string BillingOperationStatus = "ck_billing_operations_status";

    public const string BillingOperationLifecycle = "ck_billing_operations_lifecycle";

    public const string BillingOperationValidWindow =
        "ck_billing_operations_valid_window";

    public const string BillingOperationLiveAccount =
        "ux_billing_operations_live_account";

    public const string BillingWebhookExternalEvent =
        "ux_billing_webhook_events_external_event_id";

    public const string BillingWebhookEventKind =
        "ck_billing_webhook_events_kind";

    public const string BillingWebhookProcessingLease =
        "ck_billing_webhook_events_processing_lease";

    public const string BillingWebhookProcessingAttempts =
        "ck_billing_webhook_events_processing_attempts";

    public const string BillingWebhookProcessingState =
        "ck_billing_webhook_events_processing_state";

}
