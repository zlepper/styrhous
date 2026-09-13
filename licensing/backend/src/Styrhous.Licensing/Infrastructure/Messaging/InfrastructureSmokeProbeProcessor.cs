using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Application.Messaging;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed record InfrastructureSmokeProbeSettings(TimeSpan ProcessingDelay);

internal sealed class InfrastructureSmokeProbeProcessor(
    PostgresInfrastructureSmokeProbeStore store,
    IEmailSubmissionClient emailClient,
    InvitationEmailSettings emailSettings,
    InfrastructureSmokeProbeSettings settings,
    TimeProvider timeProvider)
{
    private static readonly TimeSpan LeaseMargin = TimeSpan.FromMinutes(2);
    private static readonly TimeSpan LeaseReleaseTimeout = TimeSpan.FromSeconds(5);

    public async Task<InfrastructureSmokeProbeProcessingStatus> ProcessAsync(
        Guid workId, CancellationToken cancellationToken)
    {
        var leaseId = Guid.CreateVersion7();
        var claim = await store.TryAcquireAsync(
            workId, leaseId, settings.ProcessingDelay.Add(LeaseMargin), cancellationToken);
        if (claim != InfrastructureSmokeProbeClaimStatus.Acquired)
        {
            return claim == InfrastructureSmokeProbeClaimStatus.Busy
                ? InfrastructureSmokeProbeProcessingStatus.Busy
                : InfrastructureSmokeProbeProcessingStatus.Unavailable;
        }

        try
        {
            await emailClient.SendAsync(
                new InvitationEmailSubmission(
                    workId,
                    emailSettings.FromAddress,
                    emailSettings.FromAddress,
                    "Styrhous licensing infrastructure smoke test",
                    "The licensing worker processed the deployment smoke probe.",
                    "<p>The licensing worker processed the deployment smoke probe.</p>"),
                cancellationToken);
            await Task.Delay(
                settings.ProcessingDelay,
                timeProvider,
                cancellationToken);
            if (!await store.CompleteAsync(workId, leaseId, cancellationToken))
            {
                throw new InvalidOperationException(
                    $"Infrastructure smoke probe {workId} lost its processing lease.");
            }

            return InfrastructureSmokeProbeProcessingStatus.Completed;
        }
        catch (Exception processingException)
        {
            try
            {
                using var releaseTimeout = new CancellationTokenSource(
                    LeaseReleaseTimeout);
                await store.ReleaseAsync(workId, leaseId, releaseTimeout.Token);
            }
            catch (Exception releaseException)
            {
                throw new AggregateException(
                    "Infrastructure smoke processing and lease release both failed.",
                    processingException,
                    releaseException);
            }

            throw;
        }
    }
}
