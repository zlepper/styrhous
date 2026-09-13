using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class InfrastructureSmokeProbeProcessorTests
{
    [Test]
    public async Task ProcessingSendsSandboxSafeEmailAndCompletesTheDurableProbe()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var email = new RecordingEmailClient();
        await using var test = WorkerServiceTestBase.ForDatabase<InfrastructureSmokeProbeProcessor>(
            database, DateTimeOffset.UtcNow, services =>
            {
                services.RemoveAll<IEmailSubmissionClient>();
                services.AddSingleton<IEmailSubmissionClient>(email);
            });
        var store = test.Services.GetRequiredService<PostgresInfrastructureSmokeProbeStore>();
        var probe = await store.SeedAsync(Guid.CreateVersion7(), CancellationToken.None);
        var processor = test.Service;

        await processor.ProcessAsync(probe.WorkId, CancellationToken.None);

        var persisted = await store.FindAsync(probe.ProbeId, CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(persisted!.DeliveredAt, Is.Not.Null);
            Assert.That(persisted.IsProcessing, Is.False);
            Assert.That(email.Submissions, Has.Count.EqualTo(1));
            Assert.That(email.Submissions[0].FromAddress, Is.EqualTo("invitations@example.com"));
            Assert.That(email.Submissions[0].ToAddress, Is.EqualTo("invitations@example.com"));
            Assert.That(
                email.Submissions[0].Subject,
                Is.EqualTo("Styrhous licensing infrastructure smoke test"));
        });
    }

    [Test]
    public async Task EmailFailureReleasesTheLeaseAndRetryCompletesExactlyOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var email = new FailingOnceEmailClient();
        await using var test = WorkerServiceTestBase.ForDatabase<InfrastructureSmokeProbeProcessor>(
            database, DateTimeOffset.UtcNow, services =>
            {
                services.RemoveAll<IEmailSubmissionClient>();
                services.AddSingleton<IEmailSubmissionClient>(email);
            });
        var store = test.Services.GetRequiredService<PostgresInfrastructureSmokeProbeStore>();
        var probe = await store.SeedAsync(Guid.CreateVersion7(), CancellationToken.None);
        var processor = test.Service;

        Assert.That(
            async () => await processor.ProcessAsync(probe.WorkId, CancellationToken.None),
            Throws.InvalidOperationException.With.Message.EqualTo("SES unavailable"));
        var released = await store.FindAsync(probe.ProbeId, CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(released!.IsProcessing, Is.False);
            Assert.That(released.DeliveredAt, Is.Null);
        });

        await processor.ProcessAsync(probe.WorkId, CancellationToken.None);
        await processor.ProcessAsync(probe.WorkId, CancellationToken.None);

        var delivered = await store.FindAsync(probe.ProbeId, CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(delivered!.IsProcessing, Is.False);
            Assert.That(delivered.DeliveredAt, Is.Not.Null);
            Assert.That(email.AttemptCount, Is.EqualTo(2));
        });
    }

    [Test]
    public async Task ProcessingPreservesBothDeliveryAndLeaseReleaseFailures()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var email = new FailingEmailClient(database);
        await using var test = WorkerServiceTestBase.ForDatabase<InfrastructureSmokeProbeProcessor>(
            database, DateTimeOffset.UtcNow, services =>
            {
                services.RemoveAll<IEmailSubmissionClient>();
                services.AddSingleton<IEmailSubmissionClient>(email);
            });
        var store = test.Services.GetRequiredService<PostgresInfrastructureSmokeProbeStore>();
        var probe = await store.SeedAsync(Guid.CreateVersion7(), CancellationToken.None);
        var processor = test.Service;

        Assert.That(
            async () => await processor.ProcessAsync(probe.WorkId, CancellationToken.None),
            Throws.TypeOf<AggregateException>()
                .With.Property(nameof(AggregateException.InnerExceptions))
                .Count.EqualTo(2));
    }

    private sealed class RecordingEmailClient : IEmailSubmissionClient
    {
        public List<InvitationEmailSubmission> Submissions { get; } = [];

        public Task SendAsync(
            InvitationEmailSubmission submission,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Submissions.Add(submission);
            return Task.CompletedTask;
        }
    }

    private sealed class FailingEmailClient(PostgresTestDatabase database) : IEmailSubmissionClient
    {
        public async Task SendAsync(
            InvitationEmailSubmission submission,
            CancellationToken cancellationToken)
        {
            await database.DisposeAsync();
            throw new InvalidOperationException("SES unavailable");
        }
    }

    private sealed class FailingOnceEmailClient : IEmailSubmissionClient
    {
        public int AttemptCount { get; private set; }

        public Task SendAsync(
            InvitationEmailSubmission submission,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            AttemptCount += 1;
            return AttemptCount == 1
                ? Task.FromException(new InvalidOperationException("SES unavailable"))
                : Task.CompletedTask;
        }
    }

}
