using Microsoft.EntityFrameworkCore;
using System.Text.Json;
using Microsoft.EntityFrameworkCore.Metadata.Builders;
using Microsoft.AspNetCore.DataProtection.EntityFrameworkCore;
using OpenIddict.Abstractions;
using OpenIddict.EntityFrameworkCore.Models;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Domain.Trials;

namespace Styrhous.Licensing.Persistence;

public sealed class LicensingDbContext(DbContextOptions<LicensingDbContext> options)
    : DbContext(options), IDataProtectionKeyContext
{
    public DbSet<UserAccount> UserAccounts => Set<UserAccount>();

    public DbSet<ExternalIdentity> ExternalIdentities => Set<ExternalIdentity>();

    public DbSet<VerifiedEmailClaim> VerifiedEmailClaims => Set<VerifiedEmailClaim>();

    public DbSet<BillingAccount> BillingAccounts => Set<BillingAccount>();

    public DbSet<Seat> Seats => Set<Seat>();

    public DbSet<Trial> Trials => Set<Trial>();

    public DbSet<Organization> Organizations => Set<Organization>();

    public DbSet<OrganizationMembership> OrganizationMemberships =>
        Set<OrganizationMembership>();

    public DbSet<OrganizationInvitation> OrganizationInvitations =>
        Set<OrganizationInvitation>();

    public DbSet<DeviceActivation> DeviceActivations => Set<DeviceActivation>();

    public DbSet<DesktopDeviceSession> DesktopDeviceSessions => Set<DesktopDeviceSession>();

    public DbSet<AuditRecord> AuditRecords => Set<AuditRecord>();

    public DbSet<CommercialSubscription> CommercialSubscriptions =>
        Set<CommercialSubscription>();

    public DbSet<BillingOperation> BillingOperations => Set<BillingOperation>();

    internal DbSet<BillingProviderReadCursor> BillingProviderReadCursors =>
        Set<BillingProviderReadCursor>();

    public DbSet<BillingWebhookEvent> BillingWebhookEvents =>
        Set<BillingWebhookEvent>();

    public DbSet<OutboxMessage> OutboxMessages => Set<OutboxMessage>();

    public DbSet<DataProtectionKey> DataProtectionKeys => Set<DataProtectionKey>();

    protected override void OnModelCreating(ModelBuilder modelBuilder)
    {
        base.OnModelCreating(modelBuilder);
        modelBuilder.Entity<OutboxMessage>().Property(message => message.NativeOutboxEnqueued);
        modelBuilder.Entity<BillingWebhookEvent>().Property(message => message.NativeOutboxEnqueued);
        modelBuilder.Entity<RebusOutboxMessage>(entity =>
        {
            entity.ToTable(PostgresBackgroundWorkOutbox.TableName);
            entity.HasKey(message => message.Id);
            entity.Property(message => message.Id).UseIdentityByDefaultColumn();
            entity.Property(message => message.CorrelationId).HasMaxLength(16);
            entity.Property(message => message.MessageId).HasMaxLength(255);
            entity.Property(message => message.SourceQueue).HasMaxLength(255);
            entity.Property(message => message.DestinationAddress).HasMaxLength(255).IsRequired();
            entity.Property(message => message.Sent).HasDefaultValue(false);
        });
        modelBuilder.UseOpenIddict<Guid>();
        ConfigureOpenIddict(modelBuilder);
        ConfigureUser(modelBuilder.Entity<UserAccount>());
        ConfigureExternalIdentity(modelBuilder.Entity<ExternalIdentity>());
        ConfigureVerifiedEmailClaim(modelBuilder.Entity<VerifiedEmailClaim>());
        ConfigureBillingAccount(modelBuilder.Entity<BillingAccount>());
        ConfigureSeat(modelBuilder.Entity<Seat>());
        ConfigureTrial(modelBuilder.Entity<Trial>());
        ConfigureOrganization(modelBuilder.Entity<Organization>());
        ConfigureOrganizationMembership(modelBuilder.Entity<OrganizationMembership>());
        ConfigureOrganizationInvitation(modelBuilder.Entity<OrganizationInvitation>());
        ConfigureDeviceActivation(modelBuilder.Entity<DeviceActivation>());
        ConfigureDesktopDeviceSession(modelBuilder.Entity<DesktopDeviceSession>());
        ConfigureAuditRecord(modelBuilder.Entity<AuditRecord>());
        ConfigureCommercialSubscription(modelBuilder.Entity<CommercialSubscription>());
        ConfigureBillingProviderReadCursor(modelBuilder.Entity<BillingProviderReadCursor>());
        ConfigureBillingOperation(modelBuilder.Entity<BillingOperation>());
        ConfigureBillingWebhookEvent(modelBuilder.Entity<BillingWebhookEvent>());
        ConfigureOutboxMessage(modelBuilder.Entity<OutboxMessage>());
    }

    private static void ConfigureOpenIddict(ModelBuilder modelBuilder)
    {
        var application = modelBuilder.Entity<
            OpenIddictEntityFrameworkCoreApplication<Guid>>();
        ConfigureId(application.Property(value => value.Id));
        application.HasData(
            new OpenIddictEntityFrameworkCoreApplication<Guid>
            {
                Id = Guid.Parse("01999999-0000-7000-8000-000000000001"),
                ApplicationType = OpenIddictConstants.ApplicationTypes.Native,
                ClientId = "styrhous-desktop",
                ClientType = OpenIddictConstants.ClientTypes.Public,
                ConcurrencyToken = "01999999-0000-7000-8000-000000000002",
                ConsentType = OpenIddictConstants.ConsentTypes.Implicit,
                DisplayName = "Styrhous desktop",
            });

        ConfigureId(modelBuilder.Entity<
            OpenIddictEntityFrameworkCoreAuthorization<Guid>>().Property(value => value.Id));
        ConfigureId(modelBuilder.Entity<
            OpenIddictEntityFrameworkCoreScope<Guid>>().Property(value => value.Id));
        ConfigureId(modelBuilder.Entity<
            OpenIddictEntityFrameworkCoreToken<Guid>>().Property(value => value.Id));
    }

    private static void ConfigureDesktopDeviceSession(
        EntityTypeBuilder<DesktopDeviceSession> builder)
    {
        builder.ToTable("desktop_device_sessions");
        builder.HasKey(session => session.Id).HasName("pk_desktop_device_sessions");
        ConfigureId(builder.Property(session => session.Id));
        builder.Property(session => session.ActivationId)
            .HasColumnName("activation_id")
            .IsRequired();
        builder.Property(session => session.AuthorizationId)
            .HasColumnName("authorization_id")
            .IsRequired();
        builder.Property(session => session.CreatedAt)
            .HasColumnName("created_at")
            .IsRequired();
        builder.Property(session => session.RevokedAt).HasColumnName("revoked_at");
        builder.HasIndex(session => session.AuthorizationId)
            .IsUnique()
            .HasDatabaseName("ux_desktop_device_sessions_authorization_id");
        builder.HasIndex(session => session.ActivationId)
            .HasDatabaseName("ix_desktop_device_sessions_activation_id");
        builder.HasOne<DeviceActivation>()
            .WithMany()
            .HasForeignKey(session => session.ActivationId)
            .OnDelete(DeleteBehavior.Cascade)
            .HasConstraintName("fk_desktop_device_sessions_activation_id");
        builder.HasOne<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .WithMany()
            .HasForeignKey(session => session.AuthorizationId)
            .OnDelete(DeleteBehavior.Cascade)
            .HasConstraintName("fk_desktop_device_sessions_authorization_id");
    }

    private static void ConfigureVerifiedEmailClaim(
        EntityTypeBuilder<VerifiedEmailClaim> builder)
    {
        builder.ToTable("verified_email_claims");
        builder.HasKey(claim => claim.Id).HasName("pk_verified_email_claims");
        ConfigureId(builder.Property(claim => claim.Id));
        builder.Property(claim => claim.UserId).HasColumnName("user_id").IsRequired();
        builder.Property(claim => claim.NormalizedEmail)
            .HasColumnName("normalized_email")
            .HasMaxLength(VerifiedExternalIdentity.MaximumEmailLength)
            .IsRequired();
        builder.Property(claim => claim.CreatedAt).HasColumnName("created_at").IsRequired();
        builder.HasIndex(claim => claim.NormalizedEmail)
            .IsUnique()
            .HasDatabaseName(DatabaseConstraintNames.VerifiedEmailClaimNormalizedEmail);
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(claim => claim.UserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_verified_email_claims_user_id");
    }

    private static void ConfigureUser(EntityTypeBuilder<UserAccount> builder)
    {
        builder.ToTable("user_accounts");
        builder.HasKey(user => user.Id).HasName("pk_user_accounts");
        ConfigureId(builder.Property(user => user.Id));
        builder.Property(user => user.VerifiedEmail)
            .HasColumnName("verified_email")
            .HasMaxLength(VerifiedExternalIdentity.MaximumEmailLength)
            .IsRequired();
        builder.Property(user => user.NormalizedEmail)
            .HasColumnName("normalized_email")
            .HasMaxLength(VerifiedExternalIdentity.MaximumEmailLength)
            .IsRequired();
        builder.Property(user => user.CreatedAt).HasColumnName("created_at").IsRequired();
        builder.Property(user => user.ConcurrencyVersion)
            .HasColumnName("concurrency_version")
            .HasDefaultValue(0L)
            .IsConcurrencyToken()
            .IsRequired();
    }

    private static void ConfigureExternalIdentity(EntityTypeBuilder<ExternalIdentity> builder)
    {
        builder.ToTable("external_identities");
        builder.HasKey(identity => identity.Id).HasName("pk_external_identities");
        ConfigureId(builder.Property(identity => identity.Id));
        builder.Property(identity => identity.UserId).HasColumnName("user_id").IsRequired();
        builder.Property(identity => identity.Provider)
            .HasColumnName("provider")
            .HasMaxLength(VerifiedExternalIdentity.MaximumProviderLength)
            .IsRequired();
        builder.Property(identity => identity.Subject)
            .HasColumnName("subject")
            .HasMaxLength(VerifiedExternalIdentity.MaximumSubjectLength)
            .IsRequired();
        builder.Property(identity => identity.CreatedAt).HasColumnName("created_at").IsRequired();
        builder.HasIndex(identity => new { identity.Provider, identity.Subject })
            .IsUnique()
            .HasDatabaseName(DatabaseConstraintNames.ExternalIdentityProviderSubject);
        builder.HasIndex(identity => new { identity.UserId, identity.Provider })
            .IsUnique()
            .HasDatabaseName("ux_external_identities_user_provider");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(identity => identity.UserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_external_identities_user_id");
    }

    private static void ConfigureBillingAccount(EntityTypeBuilder<BillingAccount> builder)
    {
        builder.ToTable(
            "billing_accounts",
            table => table.HasCheckConstraint(
                "ck_billing_accounts_personal_owner",
                "(kind = 'Personal' AND personal_owner_user_id IS NOT NULL) OR "
                    + "(kind = 'Organization' AND personal_owner_user_id IS NULL)"));
        builder.HasKey(account => account.Id).HasName("pk_billing_accounts");
        ConfigureId(builder.Property(account => account.Id));
        builder.Property(account => account.Kind)
            .HasColumnName("kind")
            .HasConversion<string>()
            .HasMaxLength(32)
            .IsRequired();
        builder.Property(account => account.PersonalOwnerUserId)
            .HasColumnName("personal_owner_user_id");
        builder.Property(account => account.CreatedAt).HasColumnName("created_at").IsRequired();
        builder.Property(account => account.ConcurrencyVersion)
            .HasColumnName("concurrency_version")
            .HasDefaultValue(0L)
            .IsConcurrencyToken()
            .IsRequired();
        builder.HasIndex(account => account.PersonalOwnerUserId)
            .IsUnique()
            .HasFilter("kind = 'Personal'")
            .HasDatabaseName(DatabaseConstraintNames.PersonalBillingAccountOwner);
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(account => account.PersonalOwnerUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_billing_accounts_personal_owner_user_id");
    }

    private static void ConfigureSeat(EntityTypeBuilder<Seat> builder)
    {
        builder.ToTable(
            "seats",
            table => table.HasCheckConstraint(
                DatabaseConstraintNames.SeatDeviceLimit,
                "device_limit > 0"));
        builder.HasKey(seat => seat.Id).HasName("pk_seats");
        ConfigureId(builder.Property(seat => seat.Id));
        builder.Property(seat => seat.BillingAccountId)
            .HasColumnName("billing_account_id")
            .IsRequired();
        builder.Property(seat => seat.AssignedUserId).HasColumnName("assigned_user_id").IsRequired();
        builder.Property(seat => seat.DeviceLimit)
            .HasColumnName("device_limit")
            .HasDefaultValue(Seat.DefaultDeviceLimit)
            .IsRequired();
        builder.Property(seat => seat.ProductAccessEnabled)
            .HasColumnName("product_access_enabled")
            .HasDefaultValue(true)
            .IsRequired();
        builder.Property(seat => seat.CreatedAt).HasColumnName("created_at").IsRequired();
        builder.HasIndex(seat => new { seat.BillingAccountId, seat.AssignedUserId })
            .IsUnique()
            .HasDatabaseName("ux_seats_billing_account_user");
        builder.HasIndex(seat => new { seat.BillingAccountId, seat.CreatedAt, seat.Id })
            .HasDatabaseName("ix_seats_billing_account_created_id");
        builder.HasAlternateKey(seat => new { seat.Id, seat.AssignedUserId })
            .HasName("ak_seats_id_assigned_user_id");
        builder.HasOne<BillingAccount>()
            .WithMany()
            .HasForeignKey(seat => seat.BillingAccountId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_seats_billing_account_id");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(seat => seat.AssignedUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_seats_assigned_user_id");
    }

    private static void ConfigureTrial(EntityTypeBuilder<Trial> builder)
    {
        builder.ToTable(
            "trials",
            table =>
            {
                table.HasCheckConstraint(
                    DatabaseConstraintNames.TrialValidWindow,
                    "ends_at > started_at");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.TrialTermination,
                    "terminated_at IS NULL OR "
                        + "(terminated_at >= started_at AND terminated_at < ends_at)");
            });
        builder.HasKey(trial => trial.Id).HasName("pk_trials");
        ConfigureId(builder.Property(trial => trial.Id));
        builder.Property(trial => trial.OriginatingUserId)
            .HasColumnName("originating_user_id")
            .IsRequired();
        builder.Property(trial => trial.BillingAccountId)
            .HasColumnName("billing_account_id")
            .IsRequired();
        builder.Property(trial => trial.StartedAt).HasColumnName("started_at").IsRequired();
        builder.Property(trial => trial.EndsAt).HasColumnName("ends_at").IsRequired();
        builder.Property(trial => trial.TransferredAt).HasColumnName("transferred_at");
        builder.Property(trial => trial.TerminatedAt).HasColumnName("terminated_at");
        builder.Ignore(trial => trial.EffectiveEndsAt);
        builder.HasIndex(trial => trial.OriginatingUserId)
            .IsUnique()
            .HasDatabaseName(DatabaseConstraintNames.TrialOriginatingUser);
        builder.HasIndex(trial => trial.BillingAccountId)
            .IsUnique()
            .HasDatabaseName(DatabaseConstraintNames.TrialBillingAccount);
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(trial => trial.OriginatingUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_trials_originating_user_id");
        builder.HasOne<BillingAccount>()
            .WithMany()
            .HasForeignKey(trial => trial.BillingAccountId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_trials_billing_account_id");
    }

    private static void ConfigureOrganization(EntityTypeBuilder<Organization> builder)
    {
        builder.ToTable("organizations");
        builder.HasKey(organization => organization.Id).HasName("pk_organizations");
        ConfigureId(builder.Property(organization => organization.Id));
        builder.Property(organization => organization.BillingAccountId)
            .HasColumnName("billing_account_id")
            .IsRequired();
        builder.Property(organization => organization.CreatedByUserId)
            .HasColumnName("created_by_user_id")
            .IsRequired();
        builder.Property(organization => organization.Name)
            .HasColumnName("name")
            .HasMaxLength(Organization.MaximumNameLength)
            .IsRequired();
        builder.Property(organization => organization.CreatedAt)
            .HasColumnName("created_at")
            .IsRequired();
        builder.Property(organization => organization.ConcurrencyVersion)
            .HasColumnName("concurrency_version")
            .HasDefaultValue(0L)
            .IsConcurrencyToken()
            .IsRequired();
        builder.HasIndex(organization => organization.BillingAccountId)
            .IsUnique()
            .HasDatabaseName("ux_organizations_billing_account_id");
        builder.HasIndex(organization => organization.CreatedByUserId)
            .HasDatabaseName("ix_organizations_created_by_user_id");
        builder.HasOne<BillingAccount>()
            .WithOne()
            .HasForeignKey<Organization>(organization => organization.BillingAccountId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_organizations_billing_account_id");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(organization => organization.CreatedByUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_organizations_created_by_user_id");
    }

    private static void ConfigureOrganizationMembership(
        EntityTypeBuilder<OrganizationMembership> builder)
    {
        builder.ToTable("organization_memberships");
        builder.HasKey(membership => membership.Id).HasName("pk_organization_memberships");
        ConfigureId(builder.Property(membership => membership.Id));
        builder.Property(membership => membership.OrganizationId)
            .HasColumnName("organization_id")
            .IsRequired();
        builder.Property(membership => membership.UserId)
            .HasColumnName("user_id")
            .IsRequired();
        builder.Property(membership => membership.Role)
            .HasColumnName("role")
            .HasConversion<string>()
            .HasMaxLength(32)
            .IsRequired();
        builder.Property(membership => membership.CreatedAt)
            .HasColumnName("created_at")
            .IsRequired();
        builder.HasIndex(membership => new { membership.OrganizationId, membership.UserId })
            .IsUnique()
            .HasDatabaseName("ux_organization_memberships_organization_user");
        builder.HasIndex(
                membership => new
                {
                    membership.UserId,
                    membership.CreatedAt,
                    membership.Id,
                })
            .HasDatabaseName("ix_organization_memberships_user_created_id");
        builder.HasIndex(membership => membership.OrganizationId)
            .IsUnique()
            .HasFilter("role = 'Owner'")
            .HasDatabaseName("ux_organization_memberships_single_owner");
        builder.HasOne<Organization>()
            .WithMany()
            .HasForeignKey(membership => membership.OrganizationId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_organization_memberships_organization_id");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(membership => membership.UserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_organization_memberships_user_id");
    }

    private static void ConfigureDeviceActivation(
        EntityTypeBuilder<DeviceActivation> builder)
    {
        builder.ToTable(
            "device_activations",
            table => table.HasCheckConstraint(
                DatabaseConstraintNames.DeviceActivationTimestamps,
                "last_seen_at >= activated_at AND "
                    + "((revoked_at IS NULL AND revocation_reason IS NULL) OR "
                    + "(revoked_at IS NOT NULL AND revocation_reason IS NOT NULL "
                    + "AND revoked_at >= last_seen_at))"));
        builder.HasKey(activation => activation.Id).HasName("pk_device_activations");
        ConfigureId(builder.Property(activation => activation.Id));
        builder.Property(activation => activation.SeatId).HasColumnName("seat_id").IsRequired();
        builder.Property(activation => activation.UserId).HasColumnName("user_id").IsRequired();
        builder.Property(activation => activation.InstallationId)
            .HasColumnName("installation_id")
            .IsRequired();
        builder.Property(activation => activation.DisplayName)
            .HasColumnName("display_name")
            .HasMaxLength(DesktopInstallation.MaximumDisplayNameLength)
            .IsRequired();
        builder.Property(activation => activation.Platform)
            .HasColumnName("platform")
            .HasMaxLength(DesktopInstallation.MaximumPlatformLength)
            .IsRequired();
        builder.Property(activation => activation.Architecture)
            .HasColumnName("architecture")
            .HasMaxLength(DesktopInstallation.MaximumArchitectureLength)
            .IsRequired();
        builder.Property(activation => activation.StyrhousVersion)
            .HasColumnName("styrhous_version")
            .HasMaxLength(DesktopInstallation.MaximumVersionLength)
            .IsRequired();
        builder.Property(activation => activation.ActivatedAt)
            .HasColumnName("activated_at")
            .IsRequired();
        builder.Property(activation => activation.LastSeenAt)
            .HasColumnName("last_seen_at")
            .IsRequired();
        builder.Property(activation => activation.RevokedAt).HasColumnName("revoked_at");
        builder.Property(activation => activation.RevocationReason)
            .HasColumnName("revocation_reason")
            .HasConversion<string>()
            .HasMaxLength(64);
        builder.HasIndex(activation => new { activation.SeatId, activation.InstallationId })
            .IsUnique()
            .HasFilter("revoked_at IS NULL")
            .HasDatabaseName(DatabaseConstraintNames.ActiveDeviceInstallation);
        builder.HasIndex(activation => new { activation.SeatId, activation.LastSeenAt })
            .HasFilter("revoked_at IS NULL")
            .HasDatabaseName("ix_device_activations_active_last_seen");
        builder.HasOne<Seat>()
            .WithMany()
            .HasForeignKey(activation => new { activation.SeatId, activation.UserId })
            .HasPrincipalKey(seat => new { seat.Id, seat.AssignedUserId })
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_device_activations_seat_user");
    }

    private static void ConfigureOrganizationInvitation(
        EntityTypeBuilder<OrganizationInvitation> builder)
    {
        builder.ToTable(
            "organization_invitations",
            table =>
            {
                table.HasCheckConstraint(
                    DatabaseConstraintNames.OrganizationInvitationValidWindow,
                    "last_sent_at >= created_at "
                        + "AND expires_at = last_sent_at + INTERVAL '168 hours'");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.OrganizationInvitationTerminalState,
                    "((accepted_at IS NULL AND accepted_by_user_id IS NULL) OR "
                        + "(accepted_at IS NOT NULL AND accepted_by_user_id IS NOT NULL)) "
                        + "AND NOT (accepted_at IS NOT NULL AND cancelled_at IS NOT NULL) "
                        + "AND (accepted_at IS NULL OR "
                        + "(accepted_at >= last_sent_at AND accepted_at < expires_at)) "
                        + "AND (cancelled_at IS NULL OR "
                        + "(cancelled_at >= last_sent_at AND cancelled_at < expires_at))");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.OrganizationInvitationReservedSeatCapacity,
                    "(assign_product_seat AND reserved_seat_capacity > 0) OR "
                        + "(NOT assign_product_seat AND reserved_seat_capacity = 0)");
            });
        builder.HasKey(invitation => invitation.Id)
            .HasName("pk_organization_invitations");
        ConfigureId(builder.Property(invitation => invitation.Id));
        builder.Property(invitation => invitation.OrganizationId)
            .HasColumnName("organization_id")
            .IsRequired();
        builder.Property(invitation => invitation.CreatedByUserId)
            .HasColumnName("created_by_user_id")
            .IsRequired();
        builder.Property(invitation => invitation.Email)
            .HasColumnName("email")
            .HasMaxLength(OrganizationInvitation.MaximumEmailLength)
            .IsRequired();
        builder.Property(invitation => invitation.NormalizedEmail)
            .HasColumnName("normalized_email")
            .HasMaxLength(OrganizationInvitation.MaximumEmailLength)
            .IsRequired();
        builder.Property(invitation => invitation.Role)
            .HasColumnName("role")
            .HasConversion<string>()
            .HasMaxLength(32)
            .IsRequired();
        builder.Property(invitation => invitation.SecretHash)
            .HasColumnName("secret_hash")
            .HasMaxLength(OrganizationInvitation.SecretHashLength)
            .IsFixedLength()
            .IsRequired();
        builder.Property(invitation => invitation.CreatedAt)
            .HasColumnName("created_at")
            .IsRequired();
        builder.Property(invitation => invitation.LastSentAt)
            .HasColumnName("last_sent_at")
            .IsRequired();
        builder.Property(invitation => invitation.ExpiresAt)
            .HasColumnName("expires_at")
            .IsRequired();
        builder.Property(invitation => invitation.ReservedSeatCapacity)
            .HasColumnName("reserved_seat_capacity")
            .IsRequired();
        builder.Property(invitation => invitation.AssignProductSeat)
            .HasColumnName("assign_product_seat")
            .HasDefaultValue(true)
            .IsRequired();
        builder.Property(invitation => invitation.AcceptedAt)
            .HasColumnName("accepted_at");
        builder.Property(invitation => invitation.AcceptedByUserId)
            .HasColumnName("accepted_by_user_id");
        builder.Property(invitation => invitation.CancelledAt)
            .HasColumnName("cancelled_at");
        builder.HasIndex(invitation => invitation.SecretHash)
            .IsUnique()
            .HasDatabaseName(DatabaseConstraintNames.OrganizationInvitationSecretHash);
        builder.HasIndex(
                invitation => new
                {
                    invitation.OrganizationId,
                    invitation.NormalizedEmail,
                    invitation.ExpiresAt,
                })
            .HasFilter("accepted_at IS NULL AND cancelled_at IS NULL")
            .HasDatabaseName("ix_organization_invitations_pending_email");
        builder.HasIndex(invitation => new { invitation.OrganizationId, invitation.ExpiresAt })
            .HasFilter("accepted_at IS NULL AND cancelled_at IS NULL")
            .HasDatabaseName("ix_organization_invitations_pending_expiry");
        builder.HasOne<Organization>()
            .WithMany()
            .HasForeignKey(invitation => invitation.OrganizationId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_organization_invitations_organization_id");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(invitation => invitation.CreatedByUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_organization_invitations_created_by_user_id");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(invitation => invitation.AcceptedByUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_organization_invitations_accepted_by_user_id");
    }

    private static void ConfigureAuditRecord(EntityTypeBuilder<AuditRecord> builder)
    {
        builder.ToTable("audit_records");
        builder.HasKey(record => record.Id).HasName("pk_audit_records");
        ConfigureId(builder.Property(record => record.Id));
        builder.Property(record => record.CorrelationId)
            .HasColumnName("correlation_id")
            .IsRequired();
        builder.Property(record => record.ActorUserId)
            .HasColumnName("actor_user_id")
            .IsRequired();
        builder.Property(record => record.TargetType)
            .HasColumnName("target_type")
            .HasConversion<string>()
            .HasMaxLength(64)
            .IsRequired();
        builder.Property(record => record.TargetId)
            .HasColumnName("target_id")
            .IsRequired();
        builder.Property(record => record.Action)
            .HasColumnName("action")
            .HasConversion<string>()
            .HasMaxLength(64)
            .IsRequired();
        builder.Property(record => record.OccurredAt).HasColumnName("occurred_at").IsRequired();
        builder.HasIndex(record => record.CorrelationId)
            .HasDatabaseName("ix_audit_records_correlation_id");
        builder.HasIndex(record => new { record.TargetType, record.TargetId })
            .HasDatabaseName("ix_audit_records_target");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(record => record.ActorUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_audit_records_actor_user_id");
    }

    private static void ConfigureOutboxMessage(EntityTypeBuilder<OutboxMessage> builder)
    {
        builder.ToTable(
            "outbox_messages",
            table =>
            {
                table.HasCheckConstraint(
                    DatabaseConstraintNames.OutboxLifecycle,
                    "(not_after IS NULL OR not_after > occurred_at) "
                        + "AND (delivered_at IS NULL OR delivered_at >= occurred_at) "
                        + "AND ((discarded_at IS NULL AND discard_reason IS NULL) OR "
                        + "(discarded_at IS NOT NULL AND discard_reason IS NOT NULL "
                        + "AND discarded_at >= occurred_at)) "
                        + "AND NOT (delivered_at IS NOT NULL AND discarded_at IS NOT NULL)");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.OutboxProcessingLease,
                    "(processing_lease_id IS NULL) = "
                        + "(processing_lease_expires_at IS NULL) "
                        + "AND (processing_lease_expires_at IS NULL "
                        + "OR processing_lease_expires_at > occurred_at)");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.OutboxProcessingAttempts,
                    "processing_attempt_count >= 0");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.OutboxProcessingState,
                    "(delivered_at IS NULL AND discarded_at IS NULL) "
                        + "OR processing_lease_id IS NULL");
            });
        builder.HasKey(message => message.Id).HasName("pk_outbox_messages");
        ConfigureId(builder.Property(message => message.Id));
        builder.Property(message => message.CorrelationId)
            .HasColumnName("correlation_id")
            .IsRequired();
        builder.Property(message => message.SubjectId)
            .HasColumnName("subject_id")
            .IsRequired();
        builder.Property(message => message.MessageType)
            .HasColumnName("message_type")
            .HasMaxLength(OutboxMessage.MaximumMessageTypeLength)
            .IsRequired();
        builder.Property(message => message.ProtectedPayload)
            .HasColumnName("protected_payload")
            .HasMaxLength(OutboxMessage.MaximumProtectedPayloadLength)
            .IsRequired();
        builder.Property(message => message.OccurredAt)
            .HasColumnName("occurred_at")
            .IsRequired();
        builder.Property(message => message.NotAfter).HasColumnName("not_after");
        builder.Property(message => message.DeliveredAt).HasColumnName("delivered_at");
        builder.Property(message => message.ProcessingLeaseId)
            .HasColumnName("processing_lease_id");
        builder.Property(message => message.ProcessingLeaseExpiresAt)
            .HasColumnName("processing_lease_expires_at");
        builder.Property(message => message.ProcessingAttemptCount)
            .HasColumnName("processing_attempt_count")
            .HasDefaultValue(0)
            .IsRequired();
        builder.Property(message => message.DiscardedAt).HasColumnName("discarded_at");
        builder.Property(message => message.DiscardReason)
            .HasColumnName("discard_reason")
            .HasConversion<string>()
            .HasMaxLength(64);
        builder.Property(message => message.Version)
            .HasColumnName("xmin")
            .IsRowVersion();
        builder.HasIndex(message => message.CorrelationId)
            .HasDatabaseName("ix_outbox_messages_correlation_id");
        builder.HasIndex(message => new { message.SubjectId, message.OccurredAt, message.Id })
            .HasFilter("delivered_at IS NULL AND discarded_at IS NULL")
            .HasDatabaseName("ix_outbox_messages_pending_subject_occurred_id");
    }

    private static void ConfigureCommercialSubscription(
        EntityTypeBuilder<CommercialSubscription> builder)
    {
        builder.ToTable(
            "commercial_subscriptions",
            table =>
            {
                table.HasCheckConstraint(
                    DatabaseConstraintNames.CommercialSubscriptionValidPeriod,
                    "current_period_ends_at > current_period_started_at");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.CommercialSubscriptionSeatQuantity,
                    "seat_quantity > 0");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.CommercialSubscriptionStatus,
                    "status IN ('Active', 'PastDue', 'Unpaid', 'Paused', "
                        + "'Incomplete', 'IncompleteExpired', 'Trialing', 'Canceled')");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.CommercialSubscriptionProviderReadRevision,
                    "provider_read_revision >= 0");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.CommercialSubscriptionProviderSnapshotKind,
                    "provider_snapshot_kind IN ('Observation', 'MutationResponse')");
            });
        builder.HasKey(subscription => subscription.Id)
            .HasName("pk_commercial_subscriptions");
        ConfigureId(builder.Property(subscription => subscription.Id));
        builder.Property(subscription => subscription.BillingAccountId)
            .HasColumnName("billing_account_id")
            .IsRequired();
        builder.Property(subscription => subscription.ExternalCustomerId)
            .HasColumnName("external_customer_id")
            .HasMaxLength(CommercialSubscription.MaximumExternalIdentifierLength)
            .IsRequired();
        builder.Property(subscription => subscription.ExternalSubscriptionId)
            .HasColumnName("external_subscription_id")
            .HasMaxLength(CommercialSubscription.MaximumExternalIdentifierLength)
            .IsRequired();
        builder.Property(subscription => subscription.ExternalPriceId)
            .HasColumnName("external_price_id")
            .HasMaxLength(CommercialSubscription.MaximumExternalIdentifierLength)
            .IsRequired();
        builder.Property(subscription => subscription.Status)
            .HasColumnName("status")
            .HasConversion<string>()
            .HasMaxLength(64)
            .IsRequired();
        builder.Property(subscription => subscription.SeatQuantity)
            .HasColumnName("seat_quantity")
            .IsRequired();
        builder.Property(subscription => subscription.CancelAtPeriodEnd)
            .HasColumnName("cancel_at_period_end")
            .IsRequired();
        builder.Property(subscription => subscription.CurrentPeriodStartedAt)
            .HasColumnName("current_period_started_at")
            .IsRequired();
        builder.Property(subscription => subscription.CurrentPeriodEndsAt)
            .HasColumnName("current_period_ends_at")
            .IsRequired();
        builder.Property(subscription => subscription.ProjectedAt)
            .HasColumnName("projected_at")
            .IsRequired();
        builder.Property(subscription => subscription.ProviderReadRevision)
            .HasColumnName("provider_read_revision")
            .IsRequired();
        builder.Property(subscription => subscription.ProviderSnapshotKind)
            .HasColumnName("provider_snapshot_kind")
            .HasConversion<string>()
            .HasMaxLength(32)
            .IsRequired();
        builder.Property(subscription => subscription.Version)
            .HasColumnName("xmin")
            .IsRowVersion();
        builder.HasIndex(subscription => subscription.BillingAccountId)
            .IsUnique()
            .HasDatabaseName("ux_commercial_subscriptions_billing_account_id");
        builder.HasIndex(subscription => subscription.ExternalCustomerId)
            .IsUnique()
            .HasDatabaseName("ux_commercial_subscriptions_external_customer_id");
        builder.HasIndex(subscription => subscription.ExternalSubscriptionId)
            .IsUnique()
            .HasDatabaseName("ux_commercial_subscriptions_external_subscription_id");
        builder.HasOne<BillingAccount>()
            .WithMany()
            .HasForeignKey(subscription => subscription.BillingAccountId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_commercial_subscriptions_billing_account_id");
    }

    private static void ConfigureBillingProviderReadCursor(
        EntityTypeBuilder<BillingProviderReadCursor> builder)
    {
        builder.ToTable(
            "billing_provider_read_cursors",
            table => table.HasCheckConstraint(
                DatabaseConstraintNames.BillingProviderReadCursorRevision,
                "revision >= 0"));
        builder.HasKey(cursor => cursor.Id)
            .HasName("pk_billing_provider_read_cursors");
        ConfigureId(builder.Property(cursor => cursor.Id));
        builder.Property(cursor => cursor.Revision)
            .HasColumnName("revision")
            .IsRequired();
        builder.Property(cursor => cursor.Version)
            .HasColumnName("xmin")
            .IsRowVersion();
        builder.HasData(
            new
            {
                Id = BillingProviderReadCursor.SingletonId,
                Revision = 0L,
            });
    }

    private static void ConfigureBillingOperation(
        EntityTypeBuilder<BillingOperation> builder)
    {
        builder.ToTable(
            "billing_operations",
            table =>
            {
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingOperationSeatQuantity,
                    "seat_quantity > 0 AND "
                        + "(previous_seat_quantity IS NULL OR previous_seat_quantity > 0)");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingOperationKind,
                    "kind IN ('InitialCheckout', 'SeatQuantityChange')");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingOperationCadence,
                    "(kind = 'InitialCheckout' AND cadence IN ('Monthly', 'Annual') "
                        + "AND previous_seat_quantity IS NULL) OR "
                        + "(kind = 'SeatQuantityChange' AND cadence IS NULL "
                        + "AND previous_seat_quantity IS NOT NULL "
                        + "AND previous_seat_quantity <> seat_quantity)");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingOperationStatus,
                    "status IN ('Pending', 'ProviderSessionCreated', 'Completed', "
                        + "'Failed', 'Expired')");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingOperationValidWindow,
                    "(expires_at IS NULL OR expires_at > created_at) AND "
                        + "(provider_session_recorded_at IS NULL OR "
                        + "provider_session_recorded_at >= created_at) AND "
                        + "(closed_at IS NULL OR closed_at >= created_at)");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingOperationLifecycle,
                    "(kind = 'InitialCheckout' AND expires_at IS NOT NULL "
                        + "AND provider_mutation_replay_started_at IS NULL "
                        + "AND seat_quantity_outcome IS NULL AND "
                        + "((status = 'Pending' AND provider_session_recorded_at IS NULL "
                        + "AND external_session_id IS NULL AND closed_at IS NULL) OR "
                        + "(status = 'ProviderSessionCreated' "
                        + "AND provider_session_recorded_at IS NOT NULL "
                        + "AND external_session_id IS NOT NULL AND closed_at IS NULL) OR "
                        + "(status = 'Completed' "
                        + "AND closed_at IS NOT NULL AND "
                        + "((provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL) "
                        + "OR (provider_session_recorded_at IS NULL AND external_session_id IS NULL "
                        + "AND external_subscription_id IS NOT NULL))) OR "
                        + "(status = 'Failed' AND provider_session_recorded_at IS NULL "
                        + "AND external_session_id IS NULL AND closed_at IS NOT NULL) OR "
                        + "(status = 'Expired' AND provider_session_recorded_at IS NOT NULL "
                        + "AND external_session_id IS NOT NULL AND closed_at IS NOT NULL))) OR "
                        + "(kind = 'SeatQuantityChange' AND expires_at IS NULL "
                        + "AND provider_session_recorded_at IS NULL "
                        + "AND external_session_id IS NULL AND "
                        + "((status = 'Pending' AND closed_at IS NULL "
                        + "AND seat_quantity_outcome IS NULL) OR "
                        + "(status = 'Completed' AND closed_at IS NOT NULL "
                        + "AND seat_quantity_outcome = 'Applied') OR "
                        + "(status = 'Failed' AND closed_at IS NOT NULL "
                        + "AND seat_quantity_outcome IN ('Superseded', "
                        + "'ProviderRejected'))))");
            });
        builder.HasKey(operation => operation.Id).HasName("pk_billing_operations");
        ConfigureId(builder.Property(operation => operation.Id));
        builder.Property(operation => operation.BillingAccountId)
            .HasColumnName("billing_account_id")
            .IsRequired();
        builder.Property(operation => operation.ActorUserId)
            .HasColumnName("actor_user_id")
            .IsRequired();
        builder.Property(operation => operation.Kind)
            .HasColumnName("kind")
            .HasConversion<string>()
            .HasMaxLength(64)
            .IsRequired();
        builder.Property(operation => operation.Cadence)
            .HasColumnName("cadence")
            .HasConversion<string>()
            .HasMaxLength(32);
        builder.Property(operation => operation.PreviousSeatQuantity)
            .HasColumnName("previous_seat_quantity");
        builder.Property(operation => operation.SeatQuantity)
            .HasColumnName("seat_quantity")
            .IsRequired();
        builder.Property(operation => operation.Status)
            .HasColumnName("status")
            .HasConversion<string>()
            .HasMaxLength(32)
            .IsRequired();
        builder.Property(operation => operation.CreatedAt)
            .HasColumnName("created_at")
            .IsRequired();
        builder.Property(operation => operation.ExpiresAt)
            .HasColumnName("expires_at");
        builder.Property(operation => operation.ClosedAt).HasColumnName("closed_at");
        builder.Property(operation => operation.ProviderSessionRecordedAt)
            .HasColumnName("provider_session_recorded_at");
        builder.Property(operation => operation.ExternalSessionId)
            .HasColumnName("external_session_id")
            .HasMaxLength(BillingOperation.MaximumExternalIdentifierLength);
        builder.Property(operation => operation.ExternalSubscriptionId)
            .HasColumnName("external_subscription_id")
            .HasMaxLength(BillingOperation.MaximumExternalIdentifierLength);
        builder.Property(operation => operation.PreviousSubscription)
            .HasColumnName("previous_subscription")
            .HasColumnType("jsonb")
            .HasConversion(
                snapshot => JsonSerializer.Serialize(snapshot, (JsonSerializerOptions?)null),
                json => JsonSerializer.Deserialize<CommercialSubscriptionProjection>(json, (JsonSerializerOptions?)null));
        builder.Property(operation => operation.ProviderMutationReplayStartedAt)
            .HasColumnName("provider_mutation_replay_started_at");
        builder.Property(operation => operation.SeatQuantityOutcome)
            .HasColumnName("seat_quantity_outcome")
            .HasConversion<string>()
            .HasMaxLength(32);
        builder.Property(operation => operation.Version)
            .HasColumnName("xmin")
            .IsRowVersion();
        builder.HasIndex(
                operation => new
                {
                    operation.BillingAccountId,
                    operation.CreatedAt,
                    operation.Id,
                })
            .HasDatabaseName("ix_billing_operations_account_created_id");
        builder.HasIndex(operation => operation.BillingAccountId)
            .IsUnique()
            .HasFilter("status IN ('Pending', 'ProviderSessionCreated')")
            .HasDatabaseName(DatabaseConstraintNames.BillingOperationLiveAccount);
        builder.HasIndex(operation => operation.ExternalSessionId)
            .IsUnique()
            .HasFilter("external_session_id IS NOT NULL")
            .HasDatabaseName("ux_billing_operations_external_session_id");
        builder.HasOne<BillingAccount>()
            .WithMany()
            .HasForeignKey(operation => operation.BillingAccountId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_billing_operations_billing_account_id");
        builder.HasOne<UserAccount>()
            .WithMany()
            .HasForeignKey(operation => operation.ActorUserId)
            .OnDelete(DeleteBehavior.Restrict)
            .HasConstraintName("fk_billing_operations_actor_user_id");
    }

    private static void ConfigureBillingWebhookEvent(
        EntityTypeBuilder<BillingWebhookEvent> builder)
    {
        builder.ToTable(
            "billing_webhook_events",
            table =>
            {
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingWebhookEventKind,
                    "kind IN ('CheckoutCompleted', 'SubscriptionChanged', "
                        + "'InvoicePaid', 'PaymentFailed', 'Unsupported')");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingWebhookProcessingLease,
                    "(processing_lease_id IS NULL) = "
                        + "(processing_lease_expires_at IS NULL)");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingWebhookProcessingAttempts,
                    "processing_attempt_count >= 0");
                table.HasCheckConstraint(
                    DatabaseConstraintNames.BillingWebhookProcessingState,
                    "processed_at IS NULL OR processing_lease_id IS NULL");
            });
        builder.HasKey(webhookEvent => webhookEvent.Id)
            .HasName("pk_billing_webhook_events");
        ConfigureId(builder.Property(webhookEvent => webhookEvent.Id));
        builder.Property(webhookEvent => webhookEvent.ExternalEventId)
            .HasColumnName("external_event_id")
            .HasMaxLength(BillingWebhookEvent.MaximumExternalIdentifierLength)
            .IsRequired();
        builder.Property(webhookEvent => webhookEvent.EventType)
            .HasColumnName("event_type")
            .HasMaxLength(BillingWebhookEvent.MaximumEventTypeLength)
            .IsRequired();
        builder.Property(webhookEvent => webhookEvent.Kind)
            .HasColumnName("kind")
            .HasConversion<string>()
            .HasMaxLength(64)
            .IsRequired();
        builder.Property(webhookEvent => webhookEvent.OccurredAt)
            .HasColumnName("occurred_at")
            .IsRequired();
        builder.Property(webhookEvent => webhookEvent.ReceivedAt)
            .HasColumnName("received_at")
            .IsRequired();
        builder.Property(webhookEvent => webhookEvent.ProcessedAt)
            .HasColumnName("processed_at");
        builder.Property(webhookEvent => webhookEvent.ProcessingLeaseId)
            .HasColumnName("processing_lease_id");
        builder.Property(webhookEvent => webhookEvent.ProcessingLeaseExpiresAt)
            .HasColumnName("processing_lease_expires_at");
        builder.Property(webhookEvent => webhookEvent.ProcessingAttemptCount)
            .HasColumnName("processing_attempt_count")
            .HasDefaultValue(0)
            .IsRequired();
        builder.Property(webhookEvent => webhookEvent.Version)
            .HasColumnName("xmin")
            .IsRowVersion();
        builder.HasIndex(webhookEvent => webhookEvent.ExternalEventId)
            .IsUnique()
            .HasDatabaseName(DatabaseConstraintNames.BillingWebhookExternalEvent);
        builder.HasIndex(webhookEvent => new { webhookEvent.ReceivedAt, webhookEvent.Id })
            .HasFilter("processed_at IS NULL")
            .HasDatabaseName("ix_billing_webhook_events_pending_received_id");
    }

    private static void ConfigureId(PropertyBuilder<Guid> property)
    {
        property
            .HasColumnName("id")
            .ValueGeneratedOnAdd()
            .HasValueGenerator<Uuid7ValueGenerator>();
    }
}
