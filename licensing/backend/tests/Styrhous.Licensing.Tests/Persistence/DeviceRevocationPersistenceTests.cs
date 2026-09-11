using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DeviceRevocationPersistenceTests
{
    [Test]
    public async Task ManualRevocationFreesCapacityAndWritesACorrelatedAuditRecord()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var first = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime);
        await ActivateAsync(database, signup, 3, SignupTime);

        var revocation = await RevokeAsync(
            database,
            signup.UserId,
            first.ActivationId!.Value,
            SignupTime.AddDays(1));
        var replacement = await ActivateAsync(database, signup, 4, SignupTime.AddDays(1));

        Assert.Multiple(() =>
        {
            Assert.That(revocation.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
            Assert.That(revocation.ReasonCode, Is.EqualTo(DeviceRevocationReasonCodes.Revoked));
            Assert.That(revocation.ActivationId, Is.EqualTo(first.ActivationId));
            Assert.That(revocation.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(revocation.CorrelationId.Variant, Is.InRange(8, 11));
            Assert.That(replacement.Status, Is.EqualTo(DeviceActivationStatus.Activated));
        });
        await using var context = database.CreateContext();
        var revoked = await context.DeviceActivations.SingleAsync(
            activation => activation.Id == first.ActivationId);
        var audit = await context.AuditRecords.SingleAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        Assert.Multiple(() =>
        {
            Assert.That(revoked.RevokedAt, Is.EqualTo(SignupTime.AddDays(1)));
            Assert.That(revoked.RevocationReason, Is.EqualTo(DeviceRevocationReason.Manual));
            Assert.That(
                context.DeviceActivations.Count(activation => activation.RevokedAt == null),
                Is.EqualTo(3));
            Assert.That(audit.CorrelationId, Is.EqualTo(revocation.CorrelationId));
            Assert.That(audit.TargetId, Is.EqualTo(first.ActivationId));
            Assert.That(audit.TargetType, Is.EqualTo(AuditTargetType.DeviceActivation));
            Assert.That(audit.ActorUserId, Is.EqualTo(signup.UserId));
            Assert.That(audit.OccurredAt, Is.EqualTo(SignupTime.AddDays(1)));
            Assert.That(audit.Id.Version, Is.EqualTo(7));
            Assert.That(audit.Id.Variant, Is.InRange(8, 11));
        });
    }

    [Test]
    public async Task RepeatedUnknownAndOtherUsersRevocationsShareTheInactiveResult()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "revoke-owner", "owner@example.com");
        var other = await SignUpAsync(database, "revoke-other", "other@example.com");
        var activation = await ActivateAsync(database, owner, 1, SignupTime);

        var otherUser = await RevokeAsync(
            database,
            other.UserId,
            activation.ActivationId!.Value,
            SignupTime.AddDays(1));
        var unknown = await RevokeAsync(
            database,
            owner.UserId,
            Guid.CreateVersion7(),
            SignupTime.AddDays(1));
        var revoked = await RevokeAsync(
            database,
            owner.UserId,
            activation.ActivationId.Value,
            SignupTime.AddDays(1));
        var repeated = await RevokeAsync(
            database,
            owner.UserId,
            activation.ActivationId.Value,
            SignupTime.AddDays(2));

        foreach (var result in new[] { otherUser, unknown, repeated })
        {
            Assert.Multiple(() =>
            {
                Assert.That(result.Status, Is.EqualTo(DeviceRevocationStatus.DeviceNotActive));
                Assert.That(
                    result.ReasonCode,
                    Is.EqualTo(DeviceOperationReasonCodes.DeviceNotActive));
                Assert.That(result.ActivationId, Is.Null);
            });
        }

        Assert.That(revoked.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                context.DeviceActivations.Count(activation => activation.RevokedAt != null),
                Is.EqualTo(1));
            Assert.That(
                context.AuditRecords.Count(record => record.Action == AuditAction.DeviceRevokedManual),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task RevocationAndAuditRollBackTogetherAfterSavingFails()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [new ThrowAfterSaveInterceptor()]);
        var service = revocationTest.Service;

        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await service.RevokeAsync(
                signup.UserId,
                activation.ActivationId!.Value));

        await using var context = database.CreateContext();
        var persisted = await context.DeviceActivations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.RevokedAt, Is.Null);
            Assert.That(persisted.RevocationReason, Is.Null);
            Assert.That(context.AuditRecords.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ManualRevocationRetriesTheCompleteOperationAfterConcurrencyLoss()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        var interceptor = new ConcurrencyFailureInterceptor(attempt => attempt == 1);
        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [interceptor]);
        var service = revocationTest.Service;

        var result = await service.RevokeAsync(
            signup.UserId,
            activation.ActivationId!.Value);

        await using var context = database.CreateContext();
        var persisted = await context.DeviceActivations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
            Assert.That(interceptor.AttemptCount, Is.EqualTo(2));
            Assert.That(persisted.RevokedAt, Is.EqualTo(SignupTime.AddDays(1)));
            Assert.That(
                context.AuditRecords.Count(
                    record => record.Action == AuditAction.DeviceRevokedManual),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ConcurrentRevocationAndActivationNeverExceedCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var first = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime);
        await ActivateAsync(database, signup, 3, SignupTime);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var activationFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));

        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var revocationTask = revocationTest.Service
            .RevokeAsync(signup.UserId, first.ActivationId!.Value);
        var activationTask = DevicePersistenceScenario.ActivateAsync(activationFactory, SignupTime.AddDays(1), signup.UserId, signup.SeatId, CreateInstallation(4));
        await Task.WhenAll(revocationTask, activationTask);
        var revocation = await revocationTask;
        var activation = await activationTask;

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(revocation.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
            Assert.That(
                activation.Status,
                Is.AnyOf(
                    DeviceActivationStatus.Activated,
                    DeviceActivationStatus.DeviceLimitReached));
        });
        await using var context = database.CreateContext();
        var activeCount = await context.DeviceActivations.CountAsync(
            candidate => candidate.RevokedAt == null);
        var manualAuditCount = await context.AuditRecords.CountAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        Assert.Multiple(() =>
        {
            Assert.That(
                activeCount,
                Is.EqualTo(
                    activation.Status == DeviceActivationStatus.Activated
                        ? 3
                        : 2));
            Assert.That(manualAuditCount, Is.EqualTo(1));
            Assert.That(
                context.AuditRecords.Count(),
                Is.EqualTo(
                    activation.Status == DeviceActivationStatus.Activated
                        ? 5
                        : 4));
        });
    }

    [Test]
    public async Task ConcurrentRevocationAndEntitlementCheckKeepTimestampsMonotonic()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddHours(1), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var revocationTask = revocationTest.Service
            .RevokeAsync(signup.UserId, activation.ActivationId!.Value);
        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddHours(2), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var checkTask = checkTest.Service
            .CheckAsync(signup.UserId, activation.ActivationId.Value);
        await Task.WhenAll(revocationTask, checkTask);
        var revocation = await revocationTask;
        var check = await checkTask;

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(revocation.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
            Assert.That(
                check.Status,
                Is.AnyOf(
                    DeviceEntitlementCheckStatus.Eligible,
                    DeviceEntitlementCheckStatus.DeviceNotActive));
        });
        await using var context = database.CreateContext();
        var persisted = await context.DeviceActivations.SingleAsync();
        var manualAudit = await context.AuditRecords.SingleAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        var expectedLastSeen = check.Status == DeviceEntitlementCheckStatus.Eligible
            ? SignupTime.AddHours(2)
            : SignupTime;
        var expectedRevokedAt = check.Status == DeviceEntitlementCheckStatus.Eligible
            ? SignupTime.AddHours(2)
            : SignupTime.AddHours(1);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.LastSeenAt, Is.EqualTo(expectedLastSeen));
            Assert.That(persisted.RevokedAt, Is.EqualTo(expectedRevokedAt));
            Assert.That(persisted.RevokedAt, Is.GreaterThanOrEqualTo(persisted.LastSeenAt));
            Assert.That(persisted.RevocationReason, Is.EqualTo(DeviceRevocationReason.Manual));
            Assert.That(manualAudit.OccurredAt, Is.EqualTo(expectedRevokedAt));
            Assert.That(manualAudit.CorrelationId, Is.EqualTo(revocation.CorrelationId));
        });
    }

    [Test]
    public async Task ConcurrentManualAndStaleRevocationRevokeTheTargetExactlyOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var stale = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        await ActivateAsync(database, signup, 3, SignupTime.AddDays(1));
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var activationFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));

        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(8), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var revocationTask = revocationTest.Service
            .RevokeAsync(signup.UserId, stale.ActivationId!.Value);
        var replacementTask = DevicePersistenceScenario.ActivateAsync(activationFactory, SignupTime.AddDays(8), signup.UserId, signup.SeatId, CreateInstallation(4));
        await Task.WhenAll(revocationTask, replacementTask);
        var revocation = await revocationTask;
        var replacement = await replacementTask;

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(replacement.Status, Is.EqualTo(DeviceActivationStatus.Activated));
            Assert.That(
                revocation.Status,
                Is.AnyOf(
                    DeviceRevocationStatus.Revoked,
                    DeviceRevocationStatus.DeviceNotActive));
        });
        await using var context = database.CreateContext();
        var persistedStale = await context.DeviceActivations.SingleAsync(
            candidate => candidate.Id == stale.ActivationId);
        var manualAuditCount = await context.AuditRecords.CountAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        var staleAuditCount = await context.AuditRecords.CountAsync(
            record => record.Action == AuditAction.DeviceRevokedStale);
        Assert.Multiple(() =>
        {
            Assert.That(
                context.DeviceActivations.Count(candidate => candidate.RevokedAt == null),
                Is.EqualTo(3));
            Assert.That(
                context.DeviceActivations.Count(candidate => candidate.RevokedAt != null),
                Is.EqualTo(1));
            Assert.That(
                persistedStale.RevocationReason,
                Is.EqualTo(
                    revocation.Status == DeviceRevocationStatus.Revoked
                        ? DeviceRevocationReason.Manual
                        : DeviceRevocationReason.StaleDeviceReplaced));
            Assert.That(
                replacement.RevokedActivationId,
                Is.EqualTo(
                    revocation.Status == DeviceRevocationStatus.Revoked
                        ? null
                        : stale.ActivationId));
            Assert.That(
                manualAuditCount,
                Is.EqualTo(revocation.Status == DeviceRevocationStatus.Revoked ? 1 : 0));
            Assert.That(
                staleAuditCount,
                Is.EqualTo(revocation.Status == DeviceRevocationStatus.Revoked ? 0 : 1));
            Assert.That(context.AuditRecords.Count(), Is.EqualTo(5));
        });
    }

    [Test]
    public async Task ConcurrentRepeatedRevocationCreatesOneTransitionAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        await using var revocationTest2 = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var results = await Task.WhenAll(
            revocationTest.Service
                .RevokeAsync(signup.UserId, activation.ActivationId!.Value),
            revocationTest2.Service
                .RevokeAsync(signup.UserId, activation.ActivationId.Value));

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result => result.Status == DeviceRevocationStatus.Revoked),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result => result.Status == DeviceRevocationStatus.DeviceNotActive),
                Is.EqualTo(1));
        });
        await using var context = database.CreateContext();
        var persisted = await context.DeviceActivations.SingleAsync();
        var audit = await context.AuditRecords.SingleAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        var successful = results.Single(result => result.Status == DeviceRevocationStatus.Revoked);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.RevocationReason, Is.EqualTo(DeviceRevocationReason.Manual));
            Assert.That(audit.CorrelationId, Is.EqualTo(successful.CorrelationId));
        });
    }

    [TestCase(-1)]
    [TestCase(0)]
    public async Task RevocationTimestampNeverPrecedesTheDeviceLastSeenTime(int observedTicks)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);

        var result = await RevokeAsync(
            database,
            signup.UserId,
            activation.ActivationId!.Value,
            SignupTime.AddTicks(observedTicks));

        await using var context = database.CreateContext();
        var persisted = await context.DeviceActivations.SingleAsync();
        var audit = await context.AuditRecords.SingleAsync(
            record => record.Action == AuditAction.DeviceRevokedManual);
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
            Assert.That(persisted.RevokedAt, Is.EqualTo(SignupTime));
            Assert.That(persisted.RevokedAt, Is.GreaterThanOrEqualTo(persisted.LastSeenAt));
            Assert.That(audit.OccurredAt, Is.EqualTo(SignupTime));
        });
    }

    [TestCase("user")]
    [TestCase("activation")]
    public async Task MissingOwnedIdentifiersDoNotRevealOrChangeDevices(string emptyIdentifier)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await LicensingPersistenceScenario.SignUpAsync(database);
        var activation = await DevicePersistenceScenario.ActivateAsync(database, owner, 1, SignupTime);
        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime);
        var service = revocationTest.Service;

        var result = await service.RevokeAsync(
            emptyIdentifier == "user" ? Guid.Empty : owner.UserId,
            emptyIdentifier == "activation" ? Guid.Empty : activation.ActivationId!.Value);
        Assert.That(result.Status, Is.EqualTo(DeviceRevocationStatus.DeviceNotActive));
        await using var verification = database.CreateContext();
        Assert.That((await verification.DeviceActivations.SingleAsync()).RevokedAt, Is.Null);
    }

    private static async Task<DeviceRevocationResult> RevokeAsync(
        PostgresTestDatabase database,
        Guid userId,
        Guid activationId,
        DateTimeOffset observedAt)
    {
        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, observedAt);
        return await revocationTest.Service
            .RevokeAsync(userId, activationId);
    }
}
