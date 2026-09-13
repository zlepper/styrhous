using Styrhous.Licensing.Infrastructure.Messaging;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationInvitationDeliveryStore(
    IDbContextFactory<LicensingDbContext> dbContextFactory,
    DataProtectionOrganizationInvitationDeliveryProtector deliveryProtector,
    TimeProvider timeProvider)

{

    public async Task<OrganizationInvitationDeliveryClaimResult> TryAcquireAsync(
        Guid outboxMessageId,
        Guid leaseId,
        TimeSpan leaseDuration,
        CancellationToken cancellationToken)
    {
        if (leaseDuration <= TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(
                nameof(leaseDuration),
                "The processing lease duration must be positive.");
        }

        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<
            OrganizationInvitationDeliveryClaimResult>(
            dbContext,
            async (transaction, token) =>
        {
            var message = await dbContext.OutboxMessages.SingleOrDefaultAsync(
                candidate => candidate.Id == outboxMessageId
                    && candidate.MessageType
                        == OutboxMessageTypes.OrganizationInvitationDelivery,
                cancellationToken);
            var observedAt = timeProvider.GetUtcNow();
            var terminalStatus = GetUnavailableStatus(message, observedAt);
            if (terminalStatus is not null)
            {
                return Result(terminalStatus.Value);
            }

            if (!deliveryProtector.TryUnprotect(
                    message!.ProtectedPayload,
                    out var delivery)
                || delivery is null
                || !EnvelopeMatches(message, delivery))
            {
                var discarded = await TryDiscardAndCommitAsync(
                    dbContext,
                    transaction,
                    message,
                    OutboxDiscardReason.UndeliverableProtectedPayload,
                    observedAt,
                    cancellationToken);
                return Result(
                    discarded
                        ? OrganizationInvitationDeliveryClaimStatus.Undeliverable
                        : OrganizationInvitationDeliveryClaimStatus.Busy);
            }

            var organization = await EfTransactionSerialization.FindOrganizationAndClaimAsync(
                dbContext,
                delivery.OrganizationId,
                cancellationToken);
            OrganizationInvitation? invitation = null;
            if (organization is not null)
            {
                await dbContext.Entry(message).ReloadAsync(cancellationToken);
                invitation = await EfTransactionSerialization.FindOrganizationInvitationAsync(
                    dbContext,
                    delivery.OrganizationId,
                    delivery.InvitationId,
                    cancellationToken);
            }

            var acquiredAt = timeProvider.GetUtcNow();
            terminalStatus = GetUnavailableStatus(message, acquiredAt);
            if (terminalStatus is not null)
            {
                return Result(terminalStatus.Value);
            }

            if (!MatchesCurrentInvitation(invitation, delivery, acquiredAt))
            {
                var discarded = await TryDiscardAndCommitAsync(
                    dbContext,
                    transaction,
                    message,
                    OutboxDiscardReason.Superseded,
                    acquiredAt,
                    cancellationToken);
                return Result(
                    discarded
                        ? OrganizationInvitationDeliveryClaimStatus.Discarded
                        : OrganizationInvitationDeliveryClaimStatus.Busy);
            }

            var leaseExpiresAt = acquiredAt.Add(leaseDuration);
            if (message.NotAfter is not null && message.NotAfter < leaseExpiresAt)
            {
                leaseExpiresAt = message.NotAfter.Value;
            }

            if (!message.TryAcquireProcessingLease(
                    leaseId,
                    acquiredAt,
                    leaseExpiresAt))
            {
                return Result(
                    GetUnavailableStatus(message, acquiredAt)
                        ?? OrganizationInvitationDeliveryClaimStatus.Busy);
            }

            try
            {
                await dbContext.SaveChangesAsync(cancellationToken);
                await transaction.CommitAsync(cancellationToken);
            }
            catch (DbUpdateConcurrencyException)
            {
                return Result(OrganizationInvitationDeliveryClaimStatus.Busy);
            }

            return new OrganizationInvitationDeliveryClaimResult(
                OrganizationInvitationDeliveryClaimStatus.Acquired,
                new OrganizationInvitationDeliveryClaim(
                    message.Id,
                    leaseId,
                    delivery));
        },
            cancellationToken);
    }

    public async Task<bool> CompleteAsync(
        Guid outboxMessageId,
        Guid leaseId,
        DateTimeOffset deliveredAt,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var message = await dbContext.OutboxMessages.SingleOrDefaultAsync(
            candidate => candidate.Id == outboxMessageId
                && candidate.MessageType
                    == OutboxMessageTypes.OrganizationInvitationDelivery,
            cancellationToken);
        if (message is null || !message.TryMarkDelivered(leaseId, deliveredAt))
        {
            return false;
        }

        return await TrySaveAsync(dbContext, cancellationToken);
    }

    public async Task<bool> ReleaseAsync(
        Guid outboxMessageId,
        Guid leaseId,
        CancellationToken cancellationToken)
    {
        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var message = await dbContext.OutboxMessages.SingleOrDefaultAsync(
            candidate => candidate.Id == outboxMessageId
                && candidate.MessageType
                    == OutboxMessageTypes.OrganizationInvitationDelivery,
            cancellationToken);
        if (message is null || !message.TryReleaseProcessingLease(leaseId))
        {
            return false;
        }

        return await TrySaveAsync(dbContext, cancellationToken);
    }

    private static OrganizationInvitationDeliveryClaimStatus? GetUnavailableStatus(
        OutboxMessage? message,
        DateTimeOffset acquiredAt)
    {
        if (message is null)
        {
            return OrganizationInvitationDeliveryClaimStatus.NotFound;
        }

        if (message.DeliveredAt is not null)
        {
            return OrganizationInvitationDeliveryClaimStatus.AlreadyDelivered;
        }

        if (message.DiscardedAt is not null)
        {
            return OrganizationInvitationDeliveryClaimStatus.Discarded;
        }

        if (message.NotAfter is not null && message.NotAfter <= acquiredAt)
        {
            return OrganizationInvitationDeliveryClaimStatus.Expired;
        }

        if (message.ProcessingLeaseExpiresAt is not null
            && message.ProcessingLeaseExpiresAt > acquiredAt)
        {
            return OrganizationInvitationDeliveryClaimStatus.Busy;
        }

        return null;
    }

    private static bool EnvelopeMatches(
        OutboxMessage message,
        OrganizationInvitationDelivery delivery)
    {
        return message.SubjectId == delivery.InvitationId
            && message.NotAfter == delivery.ExpiresAt;
    }

    private static bool MatchesCurrentInvitation(
        OrganizationInvitation? invitation,
        OrganizationInvitationDelivery delivery,
        DateTimeOffset acquiredAt)
    {
        if (invitation is null
            || invitation.AcceptedAt is not null
            || invitation.CancelledAt is not null
            || invitation.ExpiresAt <= acquiredAt
            || invitation.OrganizationId != delivery.OrganizationId
            || invitation.Email != delivery.Email
            || invitation.Role != delivery.Role
            || invitation.ExpiresAt != delivery.ExpiresAt)
        {
            return false;
        }

        return delivery.Secret.MatchesHash(invitation.SecretHash);
    }

    private static async Task<bool> TryDiscardAndCommitAsync(
        LicensingDbContext dbContext,
        IDbContextTransaction transaction,
        OutboxMessage message,
        OutboxDiscardReason reason,
        DateTimeOffset discardedAt,
        CancellationToken cancellationToken)
    {
        if (!message.TryDiscard(reason, discardedAt))
        {
            return false;
        }

        try
        {
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return true;
        }
        catch (DbUpdateConcurrencyException)
        {
            return false;
        }
    }

    private static async Task<bool> TrySaveAsync(
        LicensingDbContext dbContext,
        CancellationToken cancellationToken)
    {
        try
        {
            await dbContext.SaveChangesAsync(cancellationToken);
            return true;
        }
        catch (DbUpdateConcurrencyException)
        {
            return false;
        }
    }

    private static OrganizationInvitationDeliveryClaimResult Result(
        OrganizationInvitationDeliveryClaimStatus status)
    {
        return new(status, Claim: null);
    }

}
