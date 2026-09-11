using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Messaging;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresInfrastructureSmokeProbeStore(
    IDbContextFactory<LicensingDbContext> dbContextFactory,
    TimeProvider timeProvider,
    PostgresBackgroundWorkOutbox outbox)

{
    private static readonly TimeSpan ProbeLifetime = TimeSpan.FromHours(1);

    public async Task<InfrastructureSmokeProbeStatus> SeedAsync(
        Guid probeId,
        CancellationToken cancellationToken)
    {

        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        return await LicensingDbContextTransaction.ExecuteAsync<
            InfrastructureSmokeProbeStatus>(
            dbContext,
            async (transaction, token) =>
        {
            var existing = await FindMessageAsync(dbContext, probeId, cancellationToken);
            if (existing is not null)
            {
                return Status(existing);
            }

            var now = timeProvider.GetUtcNow();
            var message = OutboxMessage.Enqueue(
                probeId,
                probeId,
                OutboxMessageTypes.InfrastructureSmokeProbe,
                "infrastructure-smoke-probe",
                now,
                now.Add(ProbeLifetime));
            dbContext.OutboxMessages.Add(message);

            await outbox.EnqueueAsync(dbContext,
                BackgroundWorkReference.InfrastructureSmokeProbe(message.Id, now), cancellationToken);
            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return Status(message);
        },
            cancellationToken);
    }

    public async Task<InfrastructureSmokeProbeStatus?> FindAsync(
        Guid probeId,
        CancellationToken cancellationToken)
    {

        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var message = await FindMessageAsync(dbContext, probeId, cancellationToken);
        return message is null ? null : Status(message);
    }

    public async Task<InfrastructureSmokeProbeClaimStatus> TryAcquireAsync(
        Guid workId,
        Guid leaseId,
        TimeSpan leaseDuration,
        CancellationToken cancellationToken)
    {


        ArgumentOutOfRangeException.ThrowIfLessThanOrEqual(
            leaseDuration,
            TimeSpan.Zero);

        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var message = await FindMessageByWorkIdAsync(
            dbContext,
            workId,
            cancellationToken);
        var acquiredAt = timeProvider.GetUtcNow();
        if (message is null || message.DeliveredAt is not null || message.DiscardedAt is not null
            || message.NotAfter is not null && message.NotAfter <= acquiredAt)
        {
            return InfrastructureSmokeProbeClaimStatus.Unavailable;
        }

        if (!message.TryAcquireProcessingLease(leaseId, acquiredAt, acquiredAt.Add(leaseDuration)))
        {
            return InfrastructureSmokeProbeClaimStatus.Busy;
        }

        return await TrySaveAsync(dbContext, cancellationToken)
            ? InfrastructureSmokeProbeClaimStatus.Acquired
            : InfrastructureSmokeProbeClaimStatus.Busy;
    }

    public async Task<bool> CompleteAsync(
        Guid workId,
        Guid leaseId,
        CancellationToken cancellationToken)
    {


        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var message = await FindMessageByWorkIdAsync(
            dbContext,
            workId,
            cancellationToken);
        if (message is null
            || !message.TryMarkDelivered(leaseId, timeProvider.GetUtcNow()))
        {
            return false;
        }

        return await TrySaveAsync(dbContext, cancellationToken);
    }

    public async Task<bool> ReleaseAsync(
        Guid workId,
        Guid leaseId,
        CancellationToken cancellationToken)
    {


        await using var dbContext = await dbContextFactory.CreateDbContextAsync(
            cancellationToken);
        var message = await FindMessageByWorkIdAsync(
            dbContext,
            workId,
            cancellationToken);
        if (message is null || !message.TryReleaseProcessingLease(leaseId))
        {
            return false;
        }

        return await TrySaveAsync(dbContext, cancellationToken);
    }

    private static Task<OutboxMessage?> FindMessageAsync(
        LicensingDbContext dbContext,
        Guid probeId,
        CancellationToken cancellationToken)
    {
        return dbContext.OutboxMessages.SingleOrDefaultAsync(
            message => message.MessageType == OutboxMessageTypes.InfrastructureSmokeProbe
                && message.SubjectId == probeId,
            cancellationToken);
    }

    private static Task<OutboxMessage?> FindMessageByWorkIdAsync(
        LicensingDbContext dbContext,
        Guid workId,
        CancellationToken cancellationToken)
    {
        return dbContext.OutboxMessages.SingleOrDefaultAsync(
            message => message.MessageType == OutboxMessageTypes.InfrastructureSmokeProbe
                && message.Id == workId,
            cancellationToken);
    }

    private static InfrastructureSmokeProbeStatus Status(OutboxMessage message)
    {
        return new(
            message.SubjectId,
            message.Id,
            message.ProcessingLeaseExpiresAt is not null,
            message.DeliveredAt);
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

}
