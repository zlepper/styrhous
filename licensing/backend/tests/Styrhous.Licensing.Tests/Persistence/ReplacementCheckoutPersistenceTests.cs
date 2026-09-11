using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class ReplacementCheckoutPersistenceTests
{
    [TestCase(CommercialSubscriptionStatus.Canceled)]
    [TestCase(CommercialSubscriptionStatus.IncompleteExpired)]
    public async Task ReplacementRetainsCustomerAndHistoryAndIgnoresLaterOldSubscriptionEvents(CommercialSubscriptionStatus terminalStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var user = await SignUpAsync(database, "repurchase", "repurchase@example.com");
        var old = Projection("old", terminalStatus, SignupTime.AddDays(31));
        await using (var setup = database.CreateContext())
        {
            setup.CommercialSubscriptions.Add(CommercialSubscription.Create(user.PersonalBillingAccountId, old));
            await setup.SaveChangesAsync();
        }
        var checkoutAt = SignupTime.AddDays(32);
        await using var context = database.CreateContext();
        var store = new PostgresBillingCheckoutStore(context.CreateContextFactory());
        var prepared = await store.PrepareAsync(user.UserId, user.PersonalBillingAccountId,
            BillingCadence.Monthly, 1, null, checkoutAt, CancellationToken.None);
        var operation = prepared.Operation!;
        var retry = await store.PrepareAsync(user.UserId, user.PersonalBillingAccountId,
            BillingCadence.Monthly, 1, operation.Id, checkoutAt.AddMinutes(1), CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(retry.Operation!.Id, Is.EqualTo(operation.Id));
            Assert.That(retry.Operation.PreviousSubscription, Is.EqualTo(old));
        });
        await store.RecordProviderSessionAsync(operation.Id, "cs_replacement", checkoutAt.AddMinutes(1), CancellationToken.None);
        var replacement = new AuthoritativeCommercialSubscription(user.PersonalBillingAccountId,
            Projection("new", CommercialSubscriptionStatus.Active, checkoutAt.AddMinutes(2)),
            2, BillingOperationId: operation.Id);
        var projectionStore = new PostgresCommercialSubscriptionProjectionStore(context);
        await projectionStore.ApplyAsync(replacement, CancellationToken.None);
        await projectionStore.ApplyAsync(replacement, CancellationToken.None);
        var lateOld = new AuthoritativeCommercialSubscription(user.PersonalBillingAccountId,
            Projection("old", terminalStatus, checkoutAt.AddDays(1)), 99);
        var ignored = await projectionStore.ApplyAsync(lateOld, CancellationToken.None);
        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(ignored.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Ignored));
            Assert.That(verification.CommercialSubscriptions.Single().ExternalSubscriptionId, Is.EqualTo("sub_new"));
            Assert.That(verification.CommercialSubscriptions.Single().ExternalCustomerId, Is.EqualTo("cus_retained"));
            Assert.That(verification.BillingOperations.Single().Status, Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(verification.BillingOperations.Single().PreviousSubscription, Is.EqualTo(old));
            Assert.That(verification.Trials.Count(), Is.EqualTo(1));
            Assert.That(verification.Trials.Single().EndsAt, Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(verification.Trials.Single().TerminatedAt, Is.Null, "An already expired trial must not be restarted or extended.");
        });
    }

    [TestCase("missing-operation")]
    [TestCase("wrong-customer")]
    [TestCase("wrong-predecessor")]
    public async Task SubscriptionIdentityCannotChangeWithoutAnAuthorizedReplacement(string fault)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var user = await SignUpAsync(database, "unauthorized-repurchase", "unauthorized-repurchase@example.com");
        var at = SignupTime.AddDays(32);
        await using var context = database.CreateContext();
        var previous = Projection("old", CommercialSubscriptionStatus.Canceled, at.AddDays(-1));
        context.CommercialSubscriptions.Add(CommercialSubscription.Create(user.PersonalBillingAccountId, previous));
        var operation = BillingOperation.StartInitialCheckout(user.PersonalBillingAccountId, user.UserId,
            BillingCadence.Monthly, 1, at, fault == "wrong-predecessor"
                ? Projection("other", CommercialSubscriptionStatus.Canceled, at.AddDays(-1)) : previous);
        context.BillingOperations.Add(operation);
        await context.SaveChangesAsync();
        var projected = Projection("new", CommercialSubscriptionStatus.Active, at.AddMinutes(2),
            fault == "wrong-customer" ? "cus_wrong" : "cus_retained");
        var result = await new PostgresCommercialSubscriptionProjectionStore(context).ApplyAsync(
            new AuthoritativeCommercialSubscription(user.PersonalBillingAccountId, projected, 3,
                BillingOperationId: fault == "missing-operation" ? null : operation.Id), CancellationToken.None);
        Assert.That(result.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Ignored));
        Assert.That(context.CommercialSubscriptions.Single().ExternalSubscriptionId, Is.EqualTo("sub_old"));
    }

    [Test]
    public async Task ConcurrentPurchasesShareOneOperationAndAnEarlyWebhookClosesItAfterSessionRecording()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var user = await SignUpAsync(database, "concurrent-repurchase", "concurrent-repurchase@example.com");
        var at = SignupTime.AddDays(32);
        await using (var setup = database.CreateContext())
        {
            setup.CommercialSubscriptions.Add(CommercialSubscription.Create(user.PersonalBillingAccountId,
                Projection("old", CommercialSubscriptionStatus.Canceled, at.AddDays(-1))));
            await setup.SaveChangesAsync();
        }
        var barrier = new DatabaseCommandBarrier(2);
        async Task<BillingCheckoutPreparationResult> Prepare()
        {
            await using var connection = database.CreateContext(new DatabaseCommandBarrierInterceptor(barrier, "UPDATE billing_accounts"));
            return await new PostgresBillingCheckoutStore(connection.CreateContextFactory()).PrepareAsync(user.UserId,
                user.PersonalBillingAccountId, BillingCadence.Monthly, 1, null, at, CancellationToken.None);
        }
        var prepared = await Task.WhenAll(Prepare(), Prepare());
        Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        Assert.That(prepared[0].Operation!.Id, Is.EqualTo(prepared[1].Operation!.Id));
        var operationId = prepared[0].Operation!.Id;
        await using var context = database.CreateContext();
        await new PostgresCommercialSubscriptionProjectionStore(context).ApplyAsync(
            new AuthoritativeCommercialSubscription(user.PersonalBillingAccountId,
                Projection("new", CommercialSubscriptionStatus.Active, at.AddMinutes(1)),
                1, BillingOperationId: operationId), CancellationToken.None);
        await using var verificationContext = database.CreateContext();
        Assert.That(verificationContext.BillingOperations.Single().Status, Is.EqualTo(BillingOperationStatus.Completed),
            "A crashed Checkout request must not be needed to close the operation after an authoritative webhook.");
        await new PostgresBillingCheckoutStore(verificationContext.CreateContextFactory()).RecordProviderSessionAsync(operationId,
            "cs_late_recording", at.AddMinutes(2), CancellationToken.None);
        await using var followupVerificationContext = database.CreateContext();
        Assert.That(followupVerificationContext.BillingOperations.Single().Status, Is.EqualTo(BillingOperationStatus.Completed));
    }

    [Test]
    public async Task ProjectionObservedBeforeSessionRecordingStillCompletesAndSeatChangesDuringReplacementAreRejected()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var user = await SignUpAsync(database, "ordered-repurchase", "ordered-repurchase@example.com");
        var organization = await CreateOrganizationAsync(database, user.UserId, "Replacement ordering", SignupTime.AddDays(1));
        var at = SignupTime.AddDays(32);
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.Add(CommercialSubscription.Create(organization.BillingAccountId,
            Projection("old", CommercialSubscriptionStatus.Canceled, at.AddDays(-1))));
        await context.SaveChangesAsync();
        var checkout = new PostgresBillingCheckoutStore(context.CreateContextFactory());
        var prepared = await checkout.PrepareAsync(user.UserId, organization.BillingAccountId,
            BillingCadence.Monthly, 1, null, at, CancellationToken.None);
        var seatChange = await new PostgresBillingSeatQuantityStore(context.CreateContextFactory()).PrepareAsync(user.UserId,
            organization.BillingAccountId, 2, null, at.AddSeconds(1), CancellationToken.None);
        Assert.That(seatChange.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.SubscriptionInactive));
        await checkout.RecordProviderSessionAsync(prepared.Operation!.Id, "cs_ordered", at.AddMinutes(2), CancellationToken.None);
        await new PostgresCommercialSubscriptionProjectionStore(context).ApplyAsync(
            new AuthoritativeCommercialSubscription(organization.BillingAccountId,
                Projection("new", CommercialSubscriptionStatus.Active, at.AddMinutes(1)),
                1, BillingOperationId: prepared.Operation.Id), CancellationToken.None);
        await using var verificationContext = database.CreateContext();
        Assert.That(verificationContext.BillingOperations.Single().ClosedAt, Is.EqualTo(at.AddMinutes(2)));
    }

    private static CommercialSubscriptionProjection Projection(string id, CommercialSubscriptionStatus status,
        DateTimeOffset at, string customer = "cus_retained")
    {
        return new(customer, $"sub_{id}", "price_monthly", status, 1, false, at.AddDays(-1), at.AddDays(29), at);
    }
}
