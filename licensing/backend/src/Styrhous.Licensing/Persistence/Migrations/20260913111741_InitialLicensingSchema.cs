using System;
using Microsoft.EntityFrameworkCore.Migrations;
using Npgsql.EntityFrameworkCore.PostgreSQL.Metadata;

#nullable disable

namespace Styrhous.Licensing.Persistence.Migrations
{
    /// <inheritdoc />
    public partial class InitialLicensingSchema : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "billing_provider_read_cursors",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    revision = table.Column<long>(type: "bigint", nullable: false),
                    xmin = table.Column<uint>(type: "xid", rowVersion: true, nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_billing_provider_read_cursors", x => x.id);
                    table.CheckConstraint("ck_billing_provider_read_cursors_revision", "revision >= 0");
                });

            migrationBuilder.CreateTable(
                name: "billing_webhook_events",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    NativeOutboxEnqueued = table.Column<bool>(type: "boolean", nullable: false),
                    external_event_id = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: false),
                    event_type = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: false),
                    kind = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    occurred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    received_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    processed_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    processing_lease_id = table.Column<Guid>(type: "uuid", nullable: true),
                    processing_lease_expires_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    processing_attempt_count = table.Column<int>(type: "integer", nullable: false, defaultValue: 0),
                    xmin = table.Column<uint>(type: "xid", rowVersion: true, nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_billing_webhook_events", x => x.id);
                    table.CheckConstraint("ck_billing_webhook_events_kind", "kind IN ('CheckoutCompleted', 'SubscriptionChanged', 'InvoicePaid', 'PaymentFailed', 'Unsupported')");
                    table.CheckConstraint("ck_billing_webhook_events_processing_attempts", "processing_attempt_count >= 0");
                    table.CheckConstraint("ck_billing_webhook_events_processing_lease", "(processing_lease_id IS NULL) = (processing_lease_expires_at IS NULL)");
                    table.CheckConstraint("ck_billing_webhook_events_processing_state", "processed_at IS NULL OR processing_lease_id IS NULL");
                });

            migrationBuilder.CreateTable(
                name: "DataProtectionKeys",
                columns: table => new
                {
                    Id = table.Column<int>(type: "integer", nullable: false)
                        .Annotation("Npgsql:ValueGenerationStrategy", NpgsqlValueGenerationStrategy.IdentityByDefaultColumn),
                    FriendlyName = table.Column<string>(type: "text", nullable: true),
                    Xml = table.Column<string>(type: "text", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_DataProtectionKeys", x => x.Id);
                });

            migrationBuilder.CreateTable(
                name: "OpenIddictApplications",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    ApplicationType = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    ClientId = table.Column<string>(type: "character varying(100)", maxLength: 100, nullable: true),
                    ClientSecret = table.Column<string>(type: "text", nullable: true),
                    ClientType = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    ConcurrencyToken = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    ConsentType = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    DisplayName = table.Column<string>(type: "text", nullable: true),
                    DisplayNames = table.Column<string>(type: "text", nullable: true),
                    JsonWebKeySet = table.Column<string>(type: "text", nullable: true),
                    Permissions = table.Column<string>(type: "text", nullable: true),
                    PostLogoutRedirectUris = table.Column<string>(type: "text", nullable: true),
                    Properties = table.Column<string>(type: "text", nullable: true),
                    RedirectUris = table.Column<string>(type: "text", nullable: true),
                    Requirements = table.Column<string>(type: "text", nullable: true),
                    Settings = table.Column<string>(type: "text", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_OpenIddictApplications", x => x.id);
                });

            migrationBuilder.CreateTable(
                name: "OpenIddictScopes",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    ConcurrencyToken = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    Description = table.Column<string>(type: "text", nullable: true),
                    Descriptions = table.Column<string>(type: "text", nullable: true),
                    DisplayName = table.Column<string>(type: "text", nullable: true),
                    DisplayNames = table.Column<string>(type: "text", nullable: true),
                    Name = table.Column<string>(type: "character varying(200)", maxLength: 200, nullable: true),
                    Properties = table.Column<string>(type: "text", nullable: true),
                    Resources = table.Column<string>(type: "text", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_OpenIddictScopes", x => x.id);
                });

            migrationBuilder.CreateTable(
                name: "outbox_messages",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    NativeOutboxEnqueued = table.Column<bool>(type: "boolean", nullable: false),
                    correlation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    subject_id = table.Column<Guid>(type: "uuid", nullable: false),
                    message_type = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: false),
                    protected_payload = table.Column<string>(type: "character varying(65536)", maxLength: 65536, nullable: false),
                    occurred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    not_after = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    delivered_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    processing_lease_id = table.Column<Guid>(type: "uuid", nullable: true),
                    processing_lease_expires_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    processing_attempt_count = table.Column<int>(type: "integer", nullable: false, defaultValue: 0),
                    discarded_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    discard_reason = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true),
                    xmin = table.Column<uint>(type: "xid", rowVersion: true, nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_outbox_messages", x => x.id);
                    table.CheckConstraint("ck_outbox_messages_lifecycle", "(not_after IS NULL OR not_after > occurred_at) AND (delivered_at IS NULL OR delivered_at >= occurred_at) AND ((discarded_at IS NULL AND discard_reason IS NULL) OR (discarded_at IS NOT NULL AND discard_reason IS NOT NULL AND discarded_at >= occurred_at)) AND NOT (delivered_at IS NOT NULL AND discarded_at IS NOT NULL)");
                    table.CheckConstraint("ck_outbox_messages_processing_attempts", "processing_attempt_count >= 0");
                    table.CheckConstraint("ck_outbox_messages_processing_lease", "(processing_lease_id IS NULL) = (processing_lease_expires_at IS NULL) AND (processing_lease_expires_at IS NULL OR processing_lease_expires_at > occurred_at)");
                    table.CheckConstraint("ck_outbox_messages_processing_state", "(delivered_at IS NULL AND discarded_at IS NULL) OR processing_lease_id IS NULL");
                });

            migrationBuilder.CreateTable(
                name: "RebusOutbox",
                columns: table => new
                {
                    Id = table.Column<long>(type: "bigint", nullable: false)
                        .Annotation("Npgsql:ValueGenerationStrategy", NpgsqlValueGenerationStrategy.IdentityByDefaultColumn),
                    CorrelationId = table.Column<string>(type: "character varying(16)", maxLength: 16, nullable: true),
                    MessageId = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: true),
                    SourceQueue = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: true),
                    DestinationAddress = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: false),
                    Headers = table.Column<string>(type: "text", nullable: true),
                    Body = table.Column<byte[]>(type: "bytea", nullable: true),
                    Sent = table.Column<bool>(type: "boolean", nullable: false, defaultValue: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_RebusOutbox", x => x.Id);
                });

            migrationBuilder.CreateTable(
                name: "user_accounts",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    verified_email = table.Column<string>(type: "character varying(320)", maxLength: 320, nullable: false),
                    normalized_email = table.Column<string>(type: "character varying(320)", maxLength: 320, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    concurrency_version = table.Column<long>(type: "bigint", nullable: false, defaultValue: 0L)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_user_accounts", x => x.id);
                });

            migrationBuilder.CreateTable(
                name: "OpenIddictAuthorizations",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    ApplicationId = table.Column<Guid>(type: "uuid", nullable: true),
                    ConcurrencyToken = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    CreationDate = table.Column<DateTime>(type: "timestamp with time zone", nullable: true),
                    Properties = table.Column<string>(type: "text", nullable: true),
                    Scopes = table.Column<string>(type: "text", nullable: true),
                    Status = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    Subject = table.Column<string>(type: "character varying(400)", maxLength: 400, nullable: true),
                    Type = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_OpenIddictAuthorizations", x => x.id);
                    table.ForeignKey(
                        name: "FK_OpenIddictAuthorizations_OpenIddictApplications_Application~",
                        column: x => x.ApplicationId,
                        principalTable: "OpenIddictApplications",
                        principalColumn: "id");
                });

            migrationBuilder.CreateTable(
                name: "audit_records",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    correlation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    action = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    target_type = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    target_id = table.Column<Guid>(type: "uuid", nullable: false),
                    occurred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_audit_records", x => x.id);
                    table.ForeignKey(
                        name: "fk_audit_records_actor_user_id",
                        column: x => x.actor_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "billing_accounts",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    kind = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    personal_owner_user_id = table.Column<Guid>(type: "uuid", nullable: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    concurrency_version = table.Column<long>(type: "bigint", nullable: false, defaultValue: 0L)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_billing_accounts", x => x.id);
                    table.CheckConstraint("ck_billing_accounts_personal_owner", "(kind = 'Personal' AND personal_owner_user_id IS NOT NULL) OR (kind = 'Organization' AND personal_owner_user_id IS NULL)");
                    table.ForeignKey(
                        name: "fk_billing_accounts_personal_owner_user_id",
                        column: x => x.personal_owner_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "external_identities",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    provider = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    subject = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_external_identities", x => x.id);
                    table.ForeignKey(
                        name: "fk_external_identities_user_id",
                        column: x => x.user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "verified_email_claims",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    normalized_email = table.Column<string>(type: "character varying(320)", maxLength: 320, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_verified_email_claims", x => x.id);
                    table.ForeignKey(
                        name: "fk_verified_email_claims_user_id",
                        column: x => x.user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "OpenIddictTokens",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    ApplicationId = table.Column<Guid>(type: "uuid", nullable: true),
                    AuthorizationId = table.Column<Guid>(type: "uuid", nullable: true),
                    ConcurrencyToken = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    CreationDate = table.Column<DateTime>(type: "timestamp with time zone", nullable: true),
                    ExpirationDate = table.Column<DateTime>(type: "timestamp with time zone", nullable: true),
                    Payload = table.Column<string>(type: "text", nullable: true),
                    Properties = table.Column<string>(type: "text", nullable: true),
                    RedemptionDate = table.Column<DateTime>(type: "timestamp with time zone", nullable: true),
                    ReferenceId = table.Column<string>(type: "character varying(100)", maxLength: 100, nullable: true),
                    Status = table.Column<string>(type: "character varying(50)", maxLength: 50, nullable: true),
                    Subject = table.Column<string>(type: "character varying(400)", maxLength: 400, nullable: true),
                    Type = table.Column<string>(type: "character varying(150)", maxLength: 150, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_OpenIddictTokens", x => x.id);
                    table.ForeignKey(
                        name: "FK_OpenIddictTokens_OpenIddictApplications_ApplicationId",
                        column: x => x.ApplicationId,
                        principalTable: "OpenIddictApplications",
                        principalColumn: "id");
                    table.ForeignKey(
                        name: "FK_OpenIddictTokens_OpenIddictAuthorizations_AuthorizationId",
                        column: x => x.AuthorizationId,
                        principalTable: "OpenIddictAuthorizations",
                        principalColumn: "id");
                });

            migrationBuilder.CreateTable(
                name: "billing_operations",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    billing_account_id = table.Column<Guid>(type: "uuid", nullable: false),
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    kind = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    cadence = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: true),
                    previous_seat_quantity = table.Column<int>(type: "integer", nullable: true),
                    seat_quantity = table.Column<int>(type: "integer", nullable: false),
                    status = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    expires_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    closed_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    provider_session_recorded_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    external_session_id = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: true),
                    previous_subscription = table.Column<string>(type: "jsonb", nullable: true),
                    external_subscription_id = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: true),
                    provider_mutation_replay_started_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    seat_quantity_outcome = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: true),
                    xmin = table.Column<uint>(type: "xid", rowVersion: true, nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_billing_operations", x => x.id);
                    table.CheckConstraint("ck_billing_operations_cadence", "(kind = 'InitialCheckout' AND cadence IN ('Monthly', 'Annual') AND previous_seat_quantity IS NULL) OR (kind = 'SeatQuantityChange' AND cadence IS NULL AND previous_seat_quantity IS NOT NULL AND previous_seat_quantity <> seat_quantity)");
                    table.CheckConstraint("ck_billing_operations_kind", "kind IN ('InitialCheckout', 'SeatQuantityChange')");
                    table.CheckConstraint("ck_billing_operations_lifecycle", "(kind = 'InitialCheckout' AND expires_at IS NOT NULL AND provider_mutation_replay_started_at IS NULL AND seat_quantity_outcome IS NULL AND ((status = 'Pending' AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND closed_at IS NULL) OR (status = 'ProviderSessionCreated' AND provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL AND closed_at IS NULL) OR (status = 'Completed' AND closed_at IS NOT NULL AND ((provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL) OR (provider_session_recorded_at IS NULL AND external_session_id IS NULL AND external_subscription_id IS NOT NULL))) OR (status = 'Failed' AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND closed_at IS NOT NULL) OR (status = 'Expired' AND provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL AND closed_at IS NOT NULL))) OR (kind = 'SeatQuantityChange' AND expires_at IS NULL AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND ((status = 'Pending' AND closed_at IS NULL AND seat_quantity_outcome IS NULL) OR (status = 'Completed' AND closed_at IS NOT NULL AND seat_quantity_outcome = 'Applied') OR (status = 'Failed' AND closed_at IS NOT NULL AND seat_quantity_outcome IN ('Superseded', 'ProviderRejected'))))");
                    table.CheckConstraint("ck_billing_operations_seat_quantity", "seat_quantity > 0 AND (previous_seat_quantity IS NULL OR previous_seat_quantity > 0)");
                    table.CheckConstraint("ck_billing_operations_status", "status IN ('Pending', 'ProviderSessionCreated', 'Completed', 'Failed', 'Expired')");
                    table.CheckConstraint("ck_billing_operations_valid_window", "(expires_at IS NULL OR expires_at > created_at) AND (provider_session_recorded_at IS NULL OR provider_session_recorded_at >= created_at) AND (closed_at IS NULL OR closed_at >= created_at)");
                    table.ForeignKey(
                        name: "fk_billing_operations_actor_user_id",
                        column: x => x.actor_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "fk_billing_operations_billing_account_id",
                        column: x => x.billing_account_id,
                        principalTable: "billing_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "commercial_subscriptions",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    billing_account_id = table.Column<Guid>(type: "uuid", nullable: false),
                    external_customer_id = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: false),
                    external_subscription_id = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: false),
                    external_price_id = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: false),
                    status = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    seat_quantity = table.Column<int>(type: "integer", nullable: false),
                    cancel_at_period_end = table.Column<bool>(type: "boolean", nullable: false),
                    current_period_started_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    current_period_ends_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    projected_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    provider_read_revision = table.Column<long>(type: "bigint", nullable: false),
                    provider_snapshot_kind = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    xmin = table.Column<uint>(type: "xid", rowVersion: true, nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_commercial_subscriptions", x => x.id);
                    table.CheckConstraint("ck_commercial_subscriptions_provider_read_revision", "provider_read_revision >= 0");
                    table.CheckConstraint("ck_commercial_subscriptions_provider_snapshot_kind", "provider_snapshot_kind IN ('Observation', 'MutationResponse')");
                    table.CheckConstraint("ck_commercial_subscriptions_seat_quantity", "seat_quantity > 0");
                    table.CheckConstraint("ck_commercial_subscriptions_status", "status IN ('Active', 'PastDue', 'Unpaid', 'Paused', 'Incomplete', 'IncompleteExpired', 'Trialing', 'Canceled')");
                    table.CheckConstraint("ck_commercial_subscriptions_valid_period", "current_period_ends_at > current_period_started_at");
                    table.ForeignKey(
                        name: "fk_commercial_subscriptions_billing_account_id",
                        column: x => x.billing_account_id,
                        principalTable: "billing_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "organizations",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    billing_account_id = table.Column<Guid>(type: "uuid", nullable: false),
                    created_by_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    name = table.Column<string>(type: "character varying(120)", maxLength: 120, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    concurrency_version = table.Column<long>(type: "bigint", nullable: false, defaultValue: 0L)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_organizations", x => x.id);
                    table.ForeignKey(
                        name: "fk_organizations_billing_account_id",
                        column: x => x.billing_account_id,
                        principalTable: "billing_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "fk_organizations_created_by_user_id",
                        column: x => x.created_by_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "seats",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    billing_account_id = table.Column<Guid>(type: "uuid", nullable: false),
                    assigned_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    device_limit = table.Column<int>(type: "integer", nullable: false, defaultValue: 3),
                    product_access_enabled = table.Column<bool>(type: "boolean", nullable: false, defaultValue: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_seats", x => x.id);
                    table.UniqueConstraint("ak_seats_id_assigned_user_id", x => new { x.id, x.assigned_user_id });
                    table.CheckConstraint("ck_seats_device_limit", "device_limit > 0");
                    table.ForeignKey(
                        name: "fk_seats_assigned_user_id",
                        column: x => x.assigned_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "fk_seats_billing_account_id",
                        column: x => x.billing_account_id,
                        principalTable: "billing_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "trials",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    originating_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    billing_account_id = table.Column<Guid>(type: "uuid", nullable: false),
                    started_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    ends_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    transferred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    terminated_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_trials", x => x.id);
                    table.CheckConstraint("ck_trials_termination", "terminated_at IS NULL OR (terminated_at >= started_at AND terminated_at < ends_at)");
                    table.CheckConstraint("ck_trials_valid_window", "ends_at > started_at");
                    table.ForeignKey(
                        name: "fk_trials_billing_account_id",
                        column: x => x.billing_account_id,
                        principalTable: "billing_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "fk_trials_originating_user_id",
                        column: x => x.originating_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "organization_invitations",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    organization_id = table.Column<Guid>(type: "uuid", nullable: false),
                    created_by_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    email = table.Column<string>(type: "character varying(320)", maxLength: 320, nullable: false),
                    normalized_email = table.Column<string>(type: "character varying(320)", maxLength: 320, nullable: false),
                    role = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    secret_hash = table.Column<string>(type: "character(64)", fixedLength: true, maxLength: 64, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    last_sent_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    expires_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    reserved_seat_capacity = table.Column<int>(type: "integer", nullable: false),
                    assign_product_seat = table.Column<bool>(type: "boolean", nullable: false, defaultValue: true),
                    accepted_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    accepted_by_user_id = table.Column<Guid>(type: "uuid", nullable: true),
                    cancelled_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_organization_invitations", x => x.id);
                    table.CheckConstraint("ck_organization_invitations_reserved_seat_capacity", "(assign_product_seat AND reserved_seat_capacity > 0) OR (NOT assign_product_seat AND reserved_seat_capacity = 0)");
                    table.CheckConstraint("ck_organization_invitations_terminal_state", "((accepted_at IS NULL AND accepted_by_user_id IS NULL) OR (accepted_at IS NOT NULL AND accepted_by_user_id IS NOT NULL)) AND NOT (accepted_at IS NOT NULL AND cancelled_at IS NOT NULL) AND (accepted_at IS NULL OR (accepted_at >= last_sent_at AND accepted_at < expires_at)) AND (cancelled_at IS NULL OR (cancelled_at >= last_sent_at AND cancelled_at < expires_at))");
                    table.CheckConstraint("ck_organization_invitations_valid_window", "last_sent_at >= created_at AND expires_at = last_sent_at + INTERVAL '168 hours'");
                    table.ForeignKey(
                        name: "fk_organization_invitations_accepted_by_user_id",
                        column: x => x.accepted_by_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "fk_organization_invitations_created_by_user_id",
                        column: x => x.created_by_user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "fk_organization_invitations_organization_id",
                        column: x => x.organization_id,
                        principalTable: "organizations",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "organization_memberships",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    organization_id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    role = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_organization_memberships", x => x.id);
                    table.ForeignKey(
                        name: "fk_organization_memberships_organization_id",
                        column: x => x.organization_id,
                        principalTable: "organizations",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "fk_organization_memberships_user_id",
                        column: x => x.user_id,
                        principalTable: "user_accounts",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "device_activations",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    seat_id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    installation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    display_name = table.Column<string>(type: "character varying(120)", maxLength: 120, nullable: false),
                    platform = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    architecture = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    styrhous_version = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    activated_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    last_seen_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    revoked_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    revocation_reason = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_device_activations", x => x.id);
                    table.CheckConstraint("ck_device_activations_timestamps", "last_seen_at >= activated_at AND ((revoked_at IS NULL AND revocation_reason IS NULL) OR (revoked_at IS NOT NULL AND revocation_reason IS NOT NULL AND revoked_at >= last_seen_at))");
                    table.ForeignKey(
                        name: "fk_device_activations_seat_user",
                        columns: x => new { x.seat_id, x.user_id },
                        principalTable: "seats",
                        principalColumns: new[] { "id", "assigned_user_id" },
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "desktop_device_sessions",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    activation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    authorization_id = table.Column<Guid>(type: "uuid", nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    revoked_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("pk_desktop_device_sessions", x => x.id);
                    table.ForeignKey(
                        name: "fk_desktop_device_sessions_activation_id",
                        column: x => x.activation_id,
                        principalTable: "device_activations",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                    table.ForeignKey(
                        name: "fk_desktop_device_sessions_authorization_id",
                        column: x => x.authorization_id,
                        principalTable: "OpenIddictAuthorizations",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.InsertData(
                table: "OpenIddictApplications",
                columns: new[] { "id", "ApplicationType", "ClientId", "ClientSecret", "ClientType", "ConcurrencyToken", "ConsentType", "DisplayName", "DisplayNames", "JsonWebKeySet", "Permissions", "PostLogoutRedirectUris", "Properties", "RedirectUris", "Requirements", "Settings" },
                values: new object[] { new Guid("01999999-0000-7000-8000-000000000001"), "native", "styrhous-desktop", null, "public", "01999999-0000-7000-8000-000000000002", "implicit", "Styrhous desktop", null, null, null, null, null, null, null, null });

            migrationBuilder.InsertData(
                table: "billing_provider_read_cursors",
                columns: new[] { "id", "revision" },
                values: new object[] { new Guid("01a05c8d-9f83-74b3-9197-d3087eab0559"), 0L });

            migrationBuilder.CreateIndex(
                name: "IX_audit_records_actor_user_id",
                table: "audit_records",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "ix_audit_records_correlation_id",
                table: "audit_records",
                column: "correlation_id");

            migrationBuilder.CreateIndex(
                name: "ix_audit_records_target",
                table: "audit_records",
                columns: new[] { "target_type", "target_id" });

            migrationBuilder.CreateIndex(
                name: "ux_billing_accounts_personal_owner_user_id",
                table: "billing_accounts",
                column: "personal_owner_user_id",
                unique: true,
                filter: "kind = 'Personal'");

            migrationBuilder.CreateIndex(
                name: "ix_billing_operations_account_created_id",
                table: "billing_operations",
                columns: new[] { "billing_account_id", "created_at", "id" });

            migrationBuilder.CreateIndex(
                name: "IX_billing_operations_actor_user_id",
                table: "billing_operations",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "ux_billing_operations_external_session_id",
                table: "billing_operations",
                column: "external_session_id",
                unique: true,
                filter: "external_session_id IS NOT NULL");

            migrationBuilder.CreateIndex(
                name: "ux_billing_operations_live_account",
                table: "billing_operations",
                column: "billing_account_id",
                unique: true,
                filter: "status IN ('Pending', 'ProviderSessionCreated')");

            migrationBuilder.CreateIndex(
                name: "ix_billing_webhook_events_pending_received_id",
                table: "billing_webhook_events",
                columns: new[] { "received_at", "id" },
                filter: "processed_at IS NULL");

            migrationBuilder.CreateIndex(
                name: "ux_billing_webhook_events_external_event_id",
                table: "billing_webhook_events",
                column: "external_event_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ux_commercial_subscriptions_billing_account_id",
                table: "commercial_subscriptions",
                column: "billing_account_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ux_commercial_subscriptions_external_customer_id",
                table: "commercial_subscriptions",
                column: "external_customer_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ux_commercial_subscriptions_external_subscription_id",
                table: "commercial_subscriptions",
                column: "external_subscription_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ix_desktop_device_sessions_activation_id",
                table: "desktop_device_sessions",
                column: "activation_id");

            migrationBuilder.CreateIndex(
                name: "ux_desktop_device_sessions_authorization_id",
                table: "desktop_device_sessions",
                column: "authorization_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ix_device_activations_active_last_seen",
                table: "device_activations",
                columns: new[] { "seat_id", "last_seen_at" },
                filter: "revoked_at IS NULL");

            migrationBuilder.CreateIndex(
                name: "IX_device_activations_seat_id_user_id",
                table: "device_activations",
                columns: new[] { "seat_id", "user_id" });

            migrationBuilder.CreateIndex(
                name: "ux_device_activations_active_seat_installation",
                table: "device_activations",
                columns: new[] { "seat_id", "installation_id" },
                unique: true,
                filter: "revoked_at IS NULL");

            migrationBuilder.CreateIndex(
                name: "ux_external_identities_provider_subject",
                table: "external_identities",
                columns: new[] { "provider", "subject" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ux_external_identities_user_provider",
                table: "external_identities",
                columns: new[] { "user_id", "provider" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_OpenIddictApplications_ClientId",
                table: "OpenIddictApplications",
                column: "ClientId",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_OpenIddictAuthorizations_ApplicationId_Status_Subject_Type",
                table: "OpenIddictAuthorizations",
                columns: new[] { "ApplicationId", "Status", "Subject", "Type" });

            migrationBuilder.CreateIndex(
                name: "IX_OpenIddictScopes_Name",
                table: "OpenIddictScopes",
                column: "Name",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_OpenIddictTokens_ApplicationId_Status_Subject_Type",
                table: "OpenIddictTokens",
                columns: new[] { "ApplicationId", "Status", "Subject", "Type" });

            migrationBuilder.CreateIndex(
                name: "IX_OpenIddictTokens_AuthorizationId",
                table: "OpenIddictTokens",
                column: "AuthorizationId");

            migrationBuilder.CreateIndex(
                name: "IX_OpenIddictTokens_ReferenceId",
                table: "OpenIddictTokens",
                column: "ReferenceId",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_organization_invitations_accepted_by_user_id",
                table: "organization_invitations",
                column: "accepted_by_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_organization_invitations_created_by_user_id",
                table: "organization_invitations",
                column: "created_by_user_id");

            migrationBuilder.CreateIndex(
                name: "ix_organization_invitations_pending_email",
                table: "organization_invitations",
                columns: new[] { "organization_id", "normalized_email", "expires_at" },
                filter: "accepted_at IS NULL AND cancelled_at IS NULL");

            migrationBuilder.CreateIndex(
                name: "ix_organization_invitations_pending_expiry",
                table: "organization_invitations",
                columns: new[] { "organization_id", "expires_at" },
                filter: "accepted_at IS NULL AND cancelled_at IS NULL");

            migrationBuilder.CreateIndex(
                name: "ux_organization_invitations_secret_hash",
                table: "organization_invitations",
                column: "secret_hash",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ix_organization_memberships_user_created_id",
                table: "organization_memberships",
                columns: new[] { "user_id", "created_at", "id" });

            migrationBuilder.CreateIndex(
                name: "ux_organization_memberships_organization_user",
                table: "organization_memberships",
                columns: new[] { "organization_id", "user_id" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ux_organization_memberships_single_owner",
                table: "organization_memberships",
                column: "organization_id",
                unique: true,
                filter: "role = 'Owner'");

            migrationBuilder.CreateIndex(
                name: "ix_organizations_created_by_user_id",
                table: "organizations",
                column: "created_by_user_id");

            migrationBuilder.CreateIndex(
                name: "ux_organizations_billing_account_id",
                table: "organizations",
                column: "billing_account_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ix_outbox_messages_correlation_id",
                table: "outbox_messages",
                column: "correlation_id");

            migrationBuilder.CreateIndex(
                name: "ix_outbox_messages_pending_subject_occurred_id",
                table: "outbox_messages",
                columns: new[] { "subject_id", "occurred_at", "id" },
                filter: "delivered_at IS NULL AND discarded_at IS NULL");

            migrationBuilder.CreateIndex(
                name: "IX_seats_assigned_user_id",
                table: "seats",
                column: "assigned_user_id");

            migrationBuilder.CreateIndex(
                name: "ix_seats_billing_account_created_id",
                table: "seats",
                columns: new[] { "billing_account_id", "created_at", "id" });

            migrationBuilder.CreateIndex(
                name: "ux_seats_billing_account_user",
                table: "seats",
                columns: new[] { "billing_account_id", "assigned_user_id" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ux_trials_billing_account_id",
                table: "trials",
                column: "billing_account_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "ux_trials_originating_user_id",
                table: "trials",
                column: "originating_user_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_verified_email_claims_user_id",
                table: "verified_email_claims",
                column: "user_id");

            migrationBuilder.CreateIndex(
                name: "ux_verified_email_claims_normalized_email",
                table: "verified_email_claims",
                column: "normalized_email",
                unique: true);
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "audit_records");

            migrationBuilder.DropTable(
                name: "billing_operations");

            migrationBuilder.DropTable(
                name: "billing_provider_read_cursors");

            migrationBuilder.DropTable(
                name: "billing_webhook_events");

            migrationBuilder.DropTable(
                name: "commercial_subscriptions");

            migrationBuilder.DropTable(
                name: "DataProtectionKeys");

            migrationBuilder.DropTable(
                name: "desktop_device_sessions");

            migrationBuilder.DropTable(
                name: "external_identities");

            migrationBuilder.DropTable(
                name: "OpenIddictScopes");

            migrationBuilder.DropTable(
                name: "OpenIddictTokens");

            migrationBuilder.DropTable(
                name: "organization_invitations");

            migrationBuilder.DropTable(
                name: "organization_memberships");

            migrationBuilder.DropTable(
                name: "outbox_messages");

            migrationBuilder.DropTable(
                name: "RebusOutbox");

            migrationBuilder.DropTable(
                name: "trials");

            migrationBuilder.DropTable(
                name: "verified_email_claims");

            migrationBuilder.DropTable(
                name: "device_activations");

            migrationBuilder.DropTable(
                name: "OpenIddictAuthorizations");

            migrationBuilder.DropTable(
                name: "organizations");

            migrationBuilder.DropTable(
                name: "seats");

            migrationBuilder.DropTable(
                name: "OpenIddictApplications");

            migrationBuilder.DropTable(
                name: "billing_accounts");

            migrationBuilder.DropTable(
                name: "user_accounts");
        }
    }
}
