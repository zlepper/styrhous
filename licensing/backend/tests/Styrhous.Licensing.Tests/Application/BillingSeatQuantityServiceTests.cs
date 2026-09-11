using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingSeatQuantityServiceTests
{
    private static readonly DateTimeOffset ChangeTime = SignupTime.AddDays(3);

    [TestCase(BillingSeatQuantityProviderStatus.Applied, 4, BillingSeatQuantityStatus.Changed)]
    [TestCase(BillingSeatQuantityProviderStatus.Superseded, 5, BillingSeatQuantityStatus.SubscriptionQuantityChanged)]
    public async Task AuthoritativeOutcomeIsPersistedAndExactRetryDoesNotContactProvider(
        BillingSeatQuantityProviderStatus status, int quantity, BillingSeatQuantityStatus expected)
    {
        await using var scenario = await Scenario.CreateAsync();
        var provider = new RecordingProvider(Result(status, scenario.Operation, quantity));
        scenario.Provider = provider;
        var result = await scenario.ChangeAsync();
        await using var verification = scenario.Test.Database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        var projection = await verification.CommercialSubscriptions.SingleAsync();
        Assert.That(result.Status, Is.EqualTo(expected));
        Assert.That(result.BillingOperationId, Is.EqualTo(operation.Id));
        Assert.That(projection.SeatQuantity, Is.EqualTo(quantity));
        Assert.That(operation.Status, Is.EqualTo(status == BillingSeatQuantityProviderStatus.Applied
            ? BillingOperationStatus.Completed : BillingOperationStatus.Failed));
        scenario.Provider = new RecordingProvider(new AssertionException("Terminal retry called provider."));
        Assert.That((await scenario.ChangeAsync()).Status, Is.EqualTo(expected));
    }

    [TestCase(false)]
    [TestCase(true)]
    public async Task TransientOrIndeterminateFailureRetainsDurableRetryIdentity(bool mutation)
    {
        await using var scenario = await Scenario.CreateAsync();
        scenario.Provider = mutation
            ? new RecordingProvider(Result(BillingSeatQuantityProviderStatus.MutationRequired, scenario.Operation, 3),
                new BillingSeatQuantityProviderIndeterminateException("indeterminate", new InvalidOperationException()))
            : new RecordingProvider(new BillingSeatQuantityProviderUnavailableException("unavailable", new HttpRequestException()));
        var result = await scenario.ChangeAsync();
        await using var verification = scenario.Test.Database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        Assert.That(result.Status, Is.EqualTo(BillingSeatQuantityStatus.ProviderUnavailable));
        Assert.That(result.BillingOperationId, Is.EqualTo(operation.Id));
        Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
        Assert.That(operation.ProviderMutationReplayStartedAt.HasValue, Is.EqualTo(mutation));
    }

    [TestCase(22, 59, BillingSeatQuantityStatus.Changed)]
    [TestCase(23, 0, BillingSeatQuantityStatus.ProviderReconciliationRequired)]
    public async Task StableMutationReplayRespectsProviderKeyLifetime(int hours, int minutes,
        BillingSeatQuantityStatus expected)
    {
        await using var scenario = await Scenario.CreateAsync();
        var started = ChangeTime.AddMinutes(1);
        await scenario.StartReplayAsync(started);
        var observed = started.AddHours(hours).AddMinutes(minutes);
        var provider = new RecordingProvider(
            Result(BillingSeatQuantityProviderStatus.MutationRequired, scenario.Operation, 3, observed),
            Result(BillingSeatQuantityProviderStatus.Applied, scenario.Operation, 4, observed));
        scenario.Provider = provider;
        var result = await scenario.ChangeAsync();
        Assert.That(result.Status, Is.EqualTo(expected));
        Assert.That(provider.ApplyRequest is not null, Is.EqualTo(hours < 23));
        await using var verification = scenario.Test.Database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        Assert.That(operation.ProviderMutationReplayStartedAt, Is.EqualTo(started));
    }

    [Test]
    public async Task FreshProviderCheckAtCutoffRequiresReconciliationWithoutClosingOperation()
    {
        await using var scenario = await Scenario.CreateAsync();
        var started = ChangeTime.AddMinutes(1);
        await scenario.StartReplayAsync(started);
        var provider = new RecordingProvider(
            Result(BillingSeatQuantityProviderStatus.MutationRequired, scenario.Operation, 3, started.AddHours(22).AddMinutes(59)),
            Result(BillingSeatQuantityProviderStatus.MutationRequired, scenario.Operation, 3, started.AddHours(23)));
        scenario.Provider = provider;
        var result = await scenario.ChangeAsync();
        Assert.That(result.Status, Is.EqualTo(BillingSeatQuantityStatus.ProviderReconciliationRequired));
        Assert.That(provider.ApplyAutomaticReplayEndsAt, Is.EqualTo(started.AddHours(23)));
        await using var context = scenario.Test.Database.CreateContext();
        Assert.That((await context.BillingOperations.SingleAsync()).Status, Is.EqualTo(BillingOperationStatus.Pending));
    }

    [Test]
    public async Task OldOperationStartsReplayWindowAtFirstAuthoritativeMutationObservation()
    {
        await using var scenario = await Scenario.CreateAsync();
        var observed = ChangeTime.AddDays(2);
        scenario.Provider = new RecordingProvider(
            Result(BillingSeatQuantityProviderStatus.MutationRequired, scenario.Operation, 3, observed),
            Result(BillingSeatQuantityProviderStatus.Applied, scenario.Operation, 4, observed));
        Assert.That((await scenario.ChangeAsync()).Status, Is.EqualTo(BillingSeatQuantityStatus.Changed));
        await using var context = scenario.Test.Database.CreateContext();
        Assert.That((await context.BillingOperations.SingleAsync()).ProviderMutationReplayStartedAt, Is.EqualTo(observed));
    }

    [Test]
    public async Task NewerPersistedProviderTimeControlsAutomaticReplayCutoff()
    {
        await using var scenario = await Scenario.CreateAsync();
        var started = ChangeTime.AddMinutes(1);
        await scenario.StartReplayAsync(started);
        await using (var scope = scenario.Test.CreateScope())
        {
            await scope.ServiceProvider.GetRequiredService<CommercialSubscriptionProjectionService>()
                .ApplyAsync(scenario.Operation.BillingAccountId, Projection(3, started.AddHours(23)));
        }
        var provider = new RecordingProvider(
            Result(BillingSeatQuantityProviderStatus.MutationRequired, scenario.Operation, 3, started.AddHours(22)),
            new AssertionException("A stale observation must not replay beyond the authoritative cutoff."));
        scenario.Provider = provider;
        Assert.That((await scenario.ChangeAsync()).Status,
            Is.EqualTo(BillingSeatQuantityStatus.ProviderReconciliationRequired));
        Assert.That(provider.ApplyRequest, Is.Null);
    }

    [Test]
    public async Task StaleProviderClosureLeavesOperationRetryable()
    {
        await using var scenario = await Scenario.CreateAsync();
        await using (var scope = scenario.Test.CreateScope())
        {
            await scope.ServiceProvider.GetRequiredService<CommercialSubscriptionProjectionService>()
                .ApplyAsync(scenario.Operation.BillingAccountId, Projection(3, ChangeTime.AddMinutes(2)));
        }
        scenario.Provider = new RecordingProvider(Result(BillingSeatQuantityProviderStatus.Applied,
            scenario.Operation, 4, ChangeTime.AddMinutes(1)));
        Assert.That((await scenario.ChangeAsync()).Status, Is.EqualTo(BillingSeatQuantityStatus.ProviderUnavailable));
        await using var context = scenario.Test.Database.CreateContext();
        Assert.That((await context.BillingOperations.SingleAsync()).Status, Is.EqualTo(BillingOperationStatus.Pending));
        Assert.That((await context.CommercialSubscriptions.SingleAsync()).SeatQuantity, Is.EqualTo(3));
    }

    [Test]
    public async Task PermanentProviderRejectionIsDurableAndExactRetryReproducesFailure()
    {
        await using var scenario = await Scenario.CreateAsync();
        scenario.Provider = new RecordingProvider(new BillingSeatQuantityProviderRejectedException("rejected", new InvalidOperationException()));
        Assert.That(async () => await scenario.ChangeAsync(), Throws.TypeOf<InvalidOperationException>());
        await using var context = scenario.Test.Database.CreateContext();
        Assert.That((await context.BillingOperations.SingleAsync()).SeatQuantityOutcome, Is.EqualTo(SeatQuantityChangeOutcome.ProviderRejected));
        scenario.Provider = new RecordingProvider(new AssertionException("Terminal retry called provider."));
        Assert.That(async () => await scenario.ChangeAsync(), Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public async Task CallerCancellationLeavesExistingOperationPending()
    {
        await using var scenario = await Scenario.CreateAsync();
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        Assert.That(async () => await scenario.ChangeAsync(cancellation.Token), Throws.InstanceOf<OperationCanceledException>());
        await using var context = scenario.Test.Database.CreateContext();
        Assert.That((await context.BillingOperations.SingleAsync()).Status, Is.EqualTo(BillingOperationStatus.Pending));
    }

    [TestCase(BillingSeatQuantityProviderStatus.Applied, 7)]
    [TestCase(BillingSeatQuantityProviderStatus.MutationRequired, 4)]
    [TestCase((BillingSeatQuantityProviderStatus)int.MaxValue, 5)]
    public async Task InconsistentProviderResultDoesNotChangeProjectionOrCloseOperation(
        BillingSeatQuantityProviderStatus status, int quantity)
    {
        await using var scenario = await Scenario.CreateAsync();
        scenario.Provider = new RecordingProvider(Result(status, scenario.Operation, quantity));
        Assert.That(async () => await scenario.ChangeAsync(), Throws.TypeOf<InvalidOperationException>());
        await using var context = scenario.Test.Database.CreateContext();
        Assert.That((await context.BillingOperations.SingleAsync()).Status, Is.EqualTo(BillingOperationStatus.Pending));
        Assert.That((await context.CommercialSubscriptions.SingleAsync()).SeatQuantity, Is.EqualTo(3));
    }

    private static BillingSeatQuantityProviderResult Result(BillingSeatQuantityProviderStatus status,
        BillingOperation operation, int quantity, DateTimeOffset? observed = null)
    {
        return new(status, new AuthoritativeCommercialSubscription(operation.BillingAccountId,
            Projection(quantity, observed ?? ChangeTime.AddMinutes(1)),
            ProviderReadRevision: status == BillingSeatQuantityProviderStatus.Applied ? 2 : 1,
            ProviderSnapshotKind: status == BillingSeatQuantityProviderStatus.Applied
                ? CommercialSubscriptionSnapshotKind.MutationResponse
                : CommercialSubscriptionSnapshotKind.Observation));
    }

    private static CommercialSubscriptionProjection Projection(int quantity, DateTimeOffset observed)
    {
        return new("cus_seat_quantity_service", "sub_seat_quantity_service", "price_seat_quantity_service",
            CommercialSubscriptionStatus.Active, quantity, false, ChangeTime.AddDays(-1), ChangeTime.AddMonths(1), observed);
    }

    private sealed class Scenario : IAsyncDisposable, IBillingSeatQuantityProvider
    {
        public ServiceTestBase<BillingSeatQuantityService> Test { get; private set; } = null!;
        public BillingOperation Operation { get; private set; } = null!;
        public RecordingProvider Provider { get; set; } = null!;

        public static async Task<Scenario> CreateAsync()
        {
            var scenario = new Scenario();
            scenario.Test = await ServiceTestBase<BillingSeatQuantityService>.CreateAsync(ChangeTime.AddMinutes(5), services =>
            {
                services.RemoveAll<IBillingSeatQuantityProvider>();
                services.AddSingleton<IBillingSeatQuantityProvider>(scenario);
            });
            try
            {
                var signup = await SignUpAsync(scenario.Test.Database);
                var organization = await CreateOrganizationAsync(scenario.Test.Database, signup.UserId,
                    "Seat quantity", SignupTime.AddDays(1));
                await using var scope = scenario.Test.CreateScope();
                await scope.ServiceProvider.GetRequiredService<CommercialSubscriptionProjectionService>()
                    .ApplyAsync(organization.BillingAccountId, Projection(3, ChangeTime.AddMinutes(-1)));
                var prepared = await scope.ServiceProvider.GetRequiredService<PostgresBillingSeatQuantityStore>()
                    .PrepareAsync(signup.UserId, organization.BillingAccountId, 4, null, ChangeTime, CancellationToken.None);
                scenario.Operation = prepared.Operation!;
                Assert.That(scenario.Operation, Is.Not.Null);
                return scenario;
            }
            catch
            {
                await scenario.DisposeAsync();
                throw;
            }
        }

        public async Task StartReplayAsync(DateTimeOffset started)
        {
            await using var context = Test.Database.CreateContext();
            var operation = await context.BillingOperations.SingleAsync();
            operation.TryStartSeatQuantityProviderMutationReplay(started);
            await context.SaveChangesAsync();
        }

        public async Task<BillingSeatQuantityResult> ChangeAsync(CancellationToken token = default)
        {
            await using var scope = Test.CreateScope();
            return await scope.ServiceProvider.GetRequiredService<BillingSeatQuantityService>().ChangeAsync(
                Operation.ActorUserId, Operation.BillingAccountId, 4, Operation.Id, token);
        }

        public Task<BillingSeatQuantityProviderResult> ObserveAsync(BillingSeatQuantityProviderRequest request, CancellationToken cancellationToken)
        {
            return Provider.ObserveAsync(request, cancellationToken);
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt, CancellationToken cancellationToken)
        {
            return Provider.ApplyAsync(request, automaticReplayEndsAt, cancellationToken);
        }

        public ValueTask DisposeAsync()
        {
            return Test.DisposeAsync();
        }
    }

    private sealed class RecordingProvider : IBillingSeatQuantityProvider
    {
        private readonly BillingSeatQuantityProviderResult? _observation;
        private readonly BillingSeatQuantityProviderResult? _applyResult;
        private readonly Exception? _observationException;
        private readonly Exception? _applyException;

        public RecordingProvider(BillingSeatQuantityProviderResult result)
        {
            _observation = result;
            _applyResult = result;
        }

        public RecordingProvider(Exception exception)
        {
            _observationException = exception;
        }

        public RecordingProvider(
            BillingSeatQuantityProviderResult observation,
            Exception applyException)
        {
            _observation = observation;
            _applyException = applyException;
        }

        public RecordingProvider(
            BillingSeatQuantityProviderResult observation,
            BillingSeatQuantityProviderResult applyResult)
        {
            _observation = observation;
            _applyResult = applyResult;
        }

        public BillingSeatQuantityProviderRequest? Request { get; private set; }

        public BillingSeatQuantityProviderRequest? ApplyRequest { get; private set; }

        public DateTimeOffset? ApplyAutomaticReplayEndsAt { get; private set; }

        public Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            Request = request;
            return _observationException is null
                ? Task.FromResult(_observation!)
                : Task.FromException<BillingSeatQuantityProviderResult>(
                    _observationException);
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            ApplyRequest = request;
            ApplyAutomaticReplayEndsAt = automaticReplayEndsAt;
            return _applyException is null
                ? Task.FromResult(_applyResult!)
                : Task.FromException<BillingSeatQuantityProviderResult>(_applyException);
        }
    }

}
