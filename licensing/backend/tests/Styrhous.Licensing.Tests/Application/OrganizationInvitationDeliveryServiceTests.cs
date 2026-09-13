using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationDeliveryServiceTests
{
    private static readonly DateTimeOffset ObservedAt = SignupTime.AddDays(3);

    [Test]
    public async Task DeliverySendsCurrentInvitationAndCompletesOnce()
    {
        var sender = new RecordingSender();
        await using var test = await CreateAsync(sender);
        var id = await CreateInvitationAsync(test);
        sender.OnSend = async token =>
        {
            await using var verification = test.Database.CreateContext();
            var claimed = await verification.OutboxMessages.SingleAsync(item => item.Id == id, token);
            Assert.That(claimed.ProcessingLeaseId!.Value.Version, Is.EqualTo(7));
            Assert.That(claimed.ProcessingLeaseExpiresAt, Is.EqualTo(ObservedAt.AddMinutes(5)));
        };
        var result = await test.Service.DeliverAsync(id);
        var duplicate = await test.Service.DeliverAsync(id);
        await using var context = test.Database.CreateContext();
        var stored = await context.OutboxMessages.SingleAsync(item => item.Id == id);
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryProcessingStatus.Delivered));
            Assert.That(duplicate.Status, Is.EqualTo(OrganizationInvitationDeliveryProcessingStatus.AlreadyDelivered));
            Assert.That(sender.Calls, Is.EqualTo(1));
            Assert.That(sender.Delivery!.Email, Is.EqualTo("invitee@example.com"));
            Assert.That(sender.Id, Is.EqualTo(id));
            Assert.That(stored.DeliveredAt, Is.EqualTo(ObservedAt));
            Assert.That(stored.ProcessingLeaseId, Is.Null);
        });
    }

    [TestCase(false)]
    [TestCase(true)]
    public async Task FailedOrCancelledEmailReleasesClaimAndCanRetry(bool cancel)
    {
        using var cancellation = new CancellationTokenSource();
        var sender = new RecordingSender
        {
            OnSend = _ =>
            {
                if (cancel)
                {
                    cancellation.Cancel();
                    throw new OperationCanceledException(cancellation.Token);
                }
                throw new InvalidOperationException("email failed");
            },
        };
        await using var test = await CreateAsync(sender);
        var id = await CreateInvitationAsync(test);
        Assert.That(async () => await test.Service.DeliverAsync(id, cancellation.Token),
            cancel ? Throws.InstanceOf<OperationCanceledException>() : Throws.TypeOf<InvalidOperationException>());
        await using var context = test.Database.CreateContext();
        var stored = await context.OutboxMessages.SingleAsync(item => item.Id == id);
        Assert.That(stored.DeliveredAt, Is.Null);
        Assert.That(stored.ProcessingLeaseId, Is.Null);
        sender.OnSend = null;
        Assert.That((await test.Service.DeliverAsync(id)).Status,
            Is.EqualTo(OrganizationInvitationDeliveryProcessingStatus.Delivered));
    }

    [Test]
    public async Task LostLeaseFailsWithoutOverwritingItsNewOwner()
    {
        var sender = new RecordingSender();
        await using var test = await CreateAsync(sender);
        var id = await CreateInvitationAsync(test);
        var newOwner = Guid.CreateVersion7();
        sender.OnSend = async token =>
        {
            await using var context = test.Database.CreateContext();
            await context.OutboxMessages.Where(item => item.Id == id)
                .ExecuteUpdateAsync(setters => setters.SetProperty(item => item.ProcessingLeaseId, newOwner), token);
        };
        var exception = Assert.ThrowsAsync<InvalidOperationException>(async () => await test.Service.DeliverAsync(id));
        Assert.That(exception!.Message, Does.Contain("lease was lost"));
        await using var verification = test.Database.CreateContext();
        Assert.That((await verification.OutboxMessages.SingleAsync(item => item.Id == id)).ProcessingLeaseId,
            Is.EqualTo(newOwner));
    }

    [Test]
    public async Task ReleaseFailurePreservesOriginalEmailFailure()
    {
        var sender = new RecordingSender();
        await using var test = await CreateAsync(sender);
        var id = await CreateInvitationAsync(test);
        var failure = new InvalidOperationException("email failed");
        sender.OnSend = async _ =>
        {
            await test.Database.DisposeAsync();
            throw failure;
        };
        var exception = Assert.ThrowsAsync<AggregateException>(async () => await test.Service.DeliverAsync(id));
        Assert.That(exception!.InnerExceptions[0], Is.SameAs(failure));
        Assert.That(exception.InnerExceptions, Has.Count.EqualTo(2));
    }

    [TestCase(false)]
    [TestCase(true)]
    public async Task ExpiryBeforeOrDuringProviderSubmissionDoesNotCompleteDelivery(bool during)
    {
        var clock = new MutableTimeProvider(ObservedAt);
        var sender = new RecordingSender();
        await using var test = await CreateAsync(sender, clock);
        var id = await CreateInvitationAsync(test);
        await using var context = test.Database.CreateContext();
        var deadline = (await context.OutboxMessages.SingleAsync(item => item.Id == id)).NotAfter!.Value;
        if (during)
        {
            sender.OnSend = _ =>
            {
                clock.UtcNow = deadline;
                return Task.CompletedTask;
            };
        }
        else
        {
            clock.UtcNow = deadline;
        }
        var result = await test.Service.DeliverAsync(id);
        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryProcessingStatus.Expired));
        Assert.That(sender.Calls, Is.EqualTo(during ? 1 : 0));
        await using var verification = test.Database.CreateContext();
        var stored = await verification.OutboxMessages.SingleAsync(item => item.Id == id);
        Assert.That(stored.DeliveredAt, Is.Null);
        Assert.That(stored.ProcessingLeaseId, Is.Null);
    }

    [TestCase(false)]
    [TestCase(true)]
    public async Task MissingOrBusyInvitationDoesNotSend(bool busy)
    {
        var sender = new RecordingSender();
        await using var test = await CreateAsync(sender);
        var id = busy ? await CreateInvitationAsync(test) : Guid.NewGuid();
        if (busy)
        {
            await test.Services.GetRequiredService<PostgresOrganizationInvitationDeliveryStore>()
                .TryAcquireAsync(id, Guid.CreateVersion7(), TimeSpan.FromMinutes(5), CancellationToken.None);
        }
        var result = await test.Service.DeliverAsync(id);
        Assert.That(result.Status, Is.EqualTo(busy
            ? OrganizationInvitationDeliveryProcessingStatus.Busy : OrganizationInvitationDeliveryProcessingStatus.NotFound));
        Assert.That(sender.Calls, Is.Zero);
    }

    private static Task<ServiceTestBase<OrganizationInvitationDeliveryService>> CreateAsync(
        RecordingSender sender, TimeProvider? clock = null)
    {
        return WorkerServiceTestBase.CreateAsync<OrganizationInvitationDeliveryService>(ObservedAt, services =>
        {
            services.RemoveAll<IOrganizationInvitationEmailSender>();
            services.AddSingleton<IOrganizationInvitationEmailSender>(sender);
            if (clock is not null)
            {
                services.RemoveAll<TimeProvider>();
                services.AddSingleton(clock);
            }
        });
    }

    private static async Task<Guid> CreateInvitationAsync(ServiceTestBase<OrganizationInvitationDeliveryService> test)
    {
        var owner = await SignUpAsync(test.Database);
        var organization = await CreateOrganizationAsync(test.Database, owner.UserId, "Delivery tests", SignupTime.AddDays(1));
        await using var creation = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(test.Database, ObservedAt);
        var result = await creation.Service
            .CreateAsync(owner.UserId, organization.OrganizationId, "invitee@example.com", OrganizationRole.Member, true);
        Assert.That(result, Is.TypeOf<OrganizationInvitationCreationResult.Success>());
        await using var context = test.Database.CreateContext();
        return await context.OutboxMessages.Select(item => item.Id).SingleAsync();
    }

    private sealed class RecordingSender : IOrganizationInvitationEmailSender
    {
        public Func<CancellationToken, Task>? OnSend { get; set; }
        public int Calls { get; private set; }
        public Guid? Id { get; private set; }
        public OrganizationInvitationDelivery? Delivery { get; private set; }

        public async Task SendAsync(Guid outboxMessageId, OrganizationInvitationDelivery delivery,
            CancellationToken cancellationToken)
        {
            Calls++;
            Id = outboxMessageId;
            Delivery = delivery;
            if (OnSend is not null)
            {
                await OnSend(cancellationToken);
            }
        }
    }
}
