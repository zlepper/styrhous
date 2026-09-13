using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DeviceActivationPersistenceTests
{
    [Test]
    public async Task ActivationRetriesTheCompleteOperationAfterConcurrencyLoss()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var interceptor = new ConcurrencyFailureInterceptor(attempt => attempt == 1);


        var result = await DevicePersistenceScenario.ActivateAsync(database.CreateContextFactory(interceptor), SignupTime,
            signup.UserId,
            signup.SeatId,
            CreateInstallation(1));

        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(DeviceActivationStatus.Activated));
            Assert.That(interceptor.AttemptCount, Is.EqualTo(2));
            Assert.That(context.DeviceActivations.Count(), Is.EqualTo(1));
            Assert.That(
                context.AuditRecords.Count(
                    record => record.Action == AuditAction.DeviceActivated),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task FirstThreeDevicesActivateAndFourthRecentDeviceIsRejected()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activated = new List<DeviceActivationResult>();
        for (var index = 1; index <= 3; index++)
        {
            activated.Add(await ActivateAsync(database, signup, index, SignupTime));
        }

        var rejected = await ActivateAsync(database, signup, 4, SignupTime.AddDays(1));

        Assert.Multiple(() =>
        {
            Assert.That(activated, Has.All.Property(nameof(DeviceActivationResult.Status))
                .EqualTo(DeviceActivationStatus.Activated));
            Assert.That(
                activated.Select(result => result.ActivationId),
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
            Assert.That(rejected.Status, Is.EqualTo(DeviceActivationStatus.DeviceLimitReached));
            Assert.That(rejected.ReasonCode, Is.EqualTo(DeviceActivationReasonCodes.DeviceLimitReached));
            Assert.That(rejected.ActivationId, Is.Null);
            Assert.That(rejected.ActiveDevices, Has.Count.EqualTo(3));
        });

        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                context.DeviceActivations.Count(activation => activation.RevokedAt == null),
                Is.EqualTo(3));
            Assert.That(context.AuditRecords.Count(), Is.EqualTo(3));
            Assert.That(context.Seats.Single().DeviceLimit, Is.EqualTo(3));
        });
    }

    [Test]
    public async Task RepeatingAnActiveInstallationReturnsTheExistingActivation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var installation = CreateInstallation(1);

        var first = await ActivateAsync(database, signup, installation, SignupTime);
        var repeated = await ActivateAsync(database, signup, installation, SignupTime.AddDays(1));

        Assert.Multiple(() =>
        {
            Assert.That(first.Status, Is.EqualTo(DeviceActivationStatus.Activated));
            Assert.That(repeated.Status, Is.EqualTo(DeviceActivationStatus.AlreadyActive));
            Assert.That(repeated.ReasonCode, Is.EqualTo(DeviceActivationReasonCodes.AlreadyActive));
            Assert.That(repeated.ActivationId, Is.EqualTo(first.ActivationId));
        });
        await using var context = database.CreateContext();
        Assert.That(await context.DeviceActivations.CountAsync(), Is.EqualTo(1));
        Assert.That(await context.AuditRecords.CountAsync(), Is.EqualTo(1));
    }

    [Test]
    public async Task DeviceOlderThanSevenDaysIsEvictedForAReplacement()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var stale = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        await ActivateAsync(database, signup, 3, SignupTime.AddDays(1));

        var replacement = await ActivateAsync(database, signup, 4, SignupTime.AddDays(8));

        Assert.Multiple(() =>
        {
            Assert.That(replacement.Status, Is.EqualTo(DeviceActivationStatus.Activated));
            Assert.That(replacement.ReasonCode, Is.EqualTo(DeviceActivationReasonCodes.StaleDeviceReplaced));
            Assert.That(replacement.RevokedActivationId, Is.EqualTo(stale.ActivationId));
            Assert.That(replacement.ActiveDevices, Has.Count.EqualTo(3));
            Assert.That(replacement.ActivationId!.Value.Version, Is.EqualTo(7));
            Assert.That(replacement.CorrelationId.Version, Is.EqualTo(7));
        });
        await using var context = database.CreateContext();
        var evicted = await context.DeviceActivations.SingleAsync(
            activation => activation.Id == stale.ActivationId);
        var persistedReplacement = await context.DeviceActivations.SingleAsync(
            activation => activation.Id == replacement.ActivationId);
        var correlatedAudit = await context.AuditRecords
            .Where(record => record.CorrelationId == replacement.CorrelationId)
            .OrderBy(record => record.Action)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(evicted.RevokedAt, Is.EqualTo(SignupTime.AddDays(8)));
            Assert.That(evicted.RevocationReason, Is.EqualTo(DeviceRevocationReason.StaleDeviceReplaced));
            Assert.That(persistedReplacement.InstallationId.Version, Is.EqualTo(7));
            Assert.That(correlatedAudit, Has.Length.EqualTo(2));
            Assert.That(
                correlatedAudit.Select(record => record.Action),
                Is.EquivalentTo(new[] { AuditAction.DeviceActivated, AuditAction.DeviceRevokedStale }));
            Assert.That(
                correlatedAudit.Select(record => record.ActorUserId),
                Has.All.EqualTo(signup.UserId));
            Assert.That(
                correlatedAudit.Select(record => record.Id),
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
            Assert.That(
                correlatedAudit.Select(record => record.TargetType),
                Has.All.EqualTo(AuditTargetType.DeviceActivation));
            Assert.That(
                correlatedAudit.Select(record => record.TargetId),
                Is.EquivalentTo(new[] { stale.ActivationId, replacement.ActivationId }));
        });
    }

    [Test]
    public async Task RetryingAStaleActiveInstallationRemainsIdempotent()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var installation = CreateInstallation(1);
        var first = await ActivateAsync(database, signup, installation, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime);
        await ActivateAsync(database, signup, 3, SignupTime);

        var repeated = await ActivateAsync(
            database,
            signup,
            installation,
            SignupTime.AddDays(8));

        Assert.Multiple(() =>
        {
            Assert.That(repeated.Status, Is.EqualTo(DeviceActivationStatus.AlreadyActive));
            Assert.That(repeated.ActivationId, Is.EqualTo(first.ActivationId));
            Assert.That(repeated.RevokedActivationId, Is.Null);
        });
        await using var context = database.CreateContext();
        Assert.That(await context.DeviceActivations.CountAsync(), Is.EqualTo(3));
        Assert.That(
            await context.DeviceActivations.CountAsync(activation => activation.RevokedAt != null),
            Is.Zero);
        Assert.That(await context.AuditRecords.CountAsync(), Is.EqualTo(3));
    }

    [Test]
    public async Task ActivationReadsLastSeenChangesCommittedByAnotherContext()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var contextFactory = database.CreateContextFactory();
        var first = await DevicePersistenceScenario.ActivateAsync(contextFactory, SignupTime, signup.UserId, signup.SeatId, CreateInstallation(1));
        await DevicePersistenceScenario.ActivateAsync(contextFactory, SignupTime.AddDays(1), signup.UserId, signup.SeatId, CreateInstallation(2));
        await DevicePersistenceScenario.ActivateAsync(contextFactory, SignupTime.AddDays(1), signup.UserId, signup.SeatId, CreateInstallation(3));
        await using (var lastSeenContext = database.CreateContext())
        {
            await lastSeenContext.DeviceActivations
                .Where(activation => activation.Id == first.ActivationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    activation => activation.LastSeenAt,
                    SignupTime.AddDays(7).AddHours(12)));
        }

        var result = await DevicePersistenceScenario.ActivateAsync(contextFactory, SignupTime.AddDays(8), signup.UserId, signup.SeatId, CreateInstallation(4));

        Assert.That(result.Status, Is.EqualTo(DeviceActivationStatus.DeviceLimitReached));
        await using var verificationContext = database.CreateContext();
        Assert.That(
            await verificationContext.DeviceActivations.CountAsync(
                activation => activation.RevokedAt != null),
            Is.Zero);
    }

    [Test]
    public async Task DeviceAtExactlySevenDaysIsNotStale()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        for (var index = 1; index <= 3; index++)
        {
            await ActivateAsync(database, signup, index, SignupTime);
        }

        var result = await ActivateAsync(database, signup, 4, SignupTime.AddDays(7));

        Assert.That(result.Status, Is.EqualTo(DeviceActivationStatus.DeviceLimitReached));
        await using var context = database.CreateContext();
        Assert.That(
            await context.DeviceActivations.CountAsync(activation => activation.RevokedAt == null),
            Is.EqualTo(3));
    }

    [Test]
    public async Task LeastRecentlySeenStaleDeviceIsEvicted()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var oldest = await ActivateAsync(database, signup, 1, SignupTime);
        var middle = await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        var newest = await ActivateAsync(database, signup, 3, SignupTime.AddDays(2));

        var replacement = await ActivateAsync(database, signup, 4, SignupTime.AddDays(10));

        Assert.That(replacement.RevokedActivationId, Is.EqualTo(oldest.ActivationId));
        await using var context = database.CreateContext();
        var stillActive = await context.DeviceActivations
            .Where(activation => activation.RevokedAt == null)
            .Select(activation => activation.Id)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(stillActive, Does.Contain(middle.ActivationId));
            Assert.That(stillActive, Does.Contain(newest.ActivationId));
            Assert.That(stillActive, Does.Contain(replacement.ActivationId));
            Assert.That(stillActive, Does.Not.Contain(oldest.ActivationId));
        });
    }

    [Test]
    public async Task ConcurrentThirdAndFourthActivationNeverExceedSeatCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime);

        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var firstFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));
        var secondFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));
        var results = await Task.WhenAll(
            DevicePersistenceScenario.ActivateAsync(firstFactory, SignupTime.AddDays(1),
                signup.UserId,
                signup.SeatId,
                CreateInstallation(3)),
            DevicePersistenceScenario.ActivateAsync(secondFactory, SignupTime.AddDays(1),
                signup.UserId,
                signup.SeatId,
                CreateInstallation(4)));

        Assert.Multiple(() =>
        {
            Assert.That(results.Count(result => result.Status == DeviceActivationStatus.Activated), Is.EqualTo(1));
            Assert.That(
                results.Count(result => result.Status == DeviceActivationStatus.DeviceLimitReached),
                Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });
        await using var context = database.CreateContext();
        Assert.That(
            await context.DeviceActivations.CountAsync(activation => activation.RevokedAt == null),
            Is.EqualTo(3));
    }

    [Test]
    public async Task ConcurrentReplacementsEvictOneStaleDeviceExactlyOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var stale = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        await ActivateAsync(database, signup, 3, SignupTime.AddDays(1));

        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var firstFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));
        var secondFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));
        var results = await Task.WhenAll(
            DevicePersistenceScenario.ActivateAsync(firstFactory, SignupTime.AddDays(8),
                signup.UserId,
                signup.SeatId,
                CreateInstallation(4)),
            DevicePersistenceScenario.ActivateAsync(secondFactory, SignupTime.AddDays(8),
                signup.UserId,
                signup.SeatId,
                CreateInstallation(5)));

        var replacement = results.Single(
            result => result.Status == DeviceActivationStatus.Activated);
        Assert.Multiple(() =>
        {
            Assert.That(replacement.RevokedActivationId, Is.EqualTo(stale.ActivationId));
            Assert.That(
                results.Count(result => result.Status == DeviceActivationStatus.DeviceLimitReached),
                Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });
        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                context.DeviceActivations.Count(activation => activation.RevokedAt == null),
                Is.EqualTo(3));
            Assert.That(
                context.DeviceActivations.Count(activation => activation.RevokedAt != null),
                Is.EqualTo(1));
            Assert.That(context.DeviceActivations.Count(), Is.EqualTo(4));
            Assert.That(context.AuditRecords.Count(), Is.EqualTo(5));
        });
    }

    [Test]
    public async Task TrialTransferAndActivationUseTheSameUserLockOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var activationFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));

        var activationTask = DevicePersistenceScenario.ActivateAsync(activationFactory, SignupTime.AddDays(1), signup.UserId, signup.SeatId, CreateInstallation(1));
        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var organizationTask = organizationTest.Service
            .CreateAsync(signup.UserId, "Concurrent Transfer Organization");
        await Task.WhenAll(activationTask, organizationTask);

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(organizationTask.Result.TrialWasTransferred, Is.True);
            Assert.That(
                activationTask.Result.Status,
                Is.AnyOf(
                    DeviceActivationStatus.Activated,
                    DeviceActivationStatus.SeatNotEligible));
        });
        await using var context = database.CreateContext();
        var trial = await context.Trials.SingleAsync();
        Assert.That(
            trial.BillingAccountId,
            Is.EqualTo(organizationTask.Result.BillingAccountId));
    }

    [Test]
    public async Task ActivationAndAuditRollBackTogetherAfterSavingFails()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var factory = database.CreateContextFactory(new ThrowAfterSaveInterceptor());

        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await DevicePersistenceScenario.ActivateAsync(factory, SignupTime,
                signup.UserId,
                signup.SeatId,
                CreateInstallation(1)));

        await using var verificationContext = database.CreateContext();
        Assert.That(await verificationContext.DeviceActivations.CountAsync(), Is.Zero);
        Assert.That(await verificationContext.AuditRecords.CountAsync(), Is.Zero);
    }

    [Test]
    public async Task StaleReplacementAndBothAuditsRollBackTogetherAfterSavingFails()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var stale = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        await ActivateAsync(database, signup, 3, SignupTime.AddDays(1));
        var factory = database.CreateContextFactory(new ThrowAfterSaveInterceptor());

        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await DevicePersistenceScenario.ActivateAsync(factory, SignupTime.AddDays(8),
                signup.UserId,
                signup.SeatId,
                CreateInstallation(4)));

        await using var verificationContext = database.CreateContext();
        var original = await verificationContext.DeviceActivations.SingleAsync(
            activation => activation.Id == stale.ActivationId);
        Assert.Multiple(() =>
        {
            Assert.That(original.RevokedAt, Is.Null);
            Assert.That(original.RevocationReason, Is.Null);
            Assert.That(verificationContext.DeviceActivations.Count(), Is.EqualTo(3));
            Assert.That(verificationContext.AuditRecords.Count(), Is.EqualTo(3));
        });
    }

    [Test]
    public async Task ExpiredSeatCannotBeActivated()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database);

        var expired = await ActivateAsync(database, owner, 1, SignupTime.AddDays(30));

        Assert.Multiple(() =>
        {
            Assert.That(expired.Status, Is.EqualTo(DeviceActivationStatus.SeatNotEligible));
            Assert.That(expired.ReasonCode, Is.EqualTo(DeviceActivationReasonCodes.SeatNotEligible));
        });
        await using var context = database.CreateContext();
        Assert.That(await context.DeviceActivations.CountAsync(), Is.Zero);
        Assert.That(await context.AuditRecords.CountAsync(), Is.Zero);
    }

    [Test]
    public async Task OtherUserCannotSeeOrMutateTheOwnersActiveDevices()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "activation-owner", "owner@example.com");
        var otherUser = await SignUpAsync(database, "activation-other", "other@example.com");
        await ActivateAsync(database, owner, 1, SignupTime);
        await ActivateAsync(database, owner, 2, SignupTime);

        var wrongUser = await DevicePersistenceScenario.ActivateAsync(database.CreateContextFactory(), SignupTime,
            otherUser.UserId,
            owner.SeatId,
            CreateInstallation(3));

        await using var context = database.CreateContext();
        var activationCount = await context.DeviceActivations.CountAsync();
        var auditCount = await context.AuditRecords.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(wrongUser.Status, Is.EqualTo(DeviceActivationStatus.SeatNotEligible));
            Assert.That(wrongUser.ActiveDevices, Is.Empty);
            Assert.That(activationCount, Is.EqualTo(2));
            Assert.That(auditCount, Is.EqualTo(2));
        });
    }

    [Test]
    public async Task DeviceLimitsAreIndependentForEachEligibleSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-owner", "owner@example.com");
        var member = await SignUpAsync(database, "seat-member", "member@example.com");
        OrganizationCreationResult organization;
        await using (var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
                database, SignupTime.AddDays(1)))
        {
            organization = await organizationTest.Service
                .CreateAsync(owner.UserId, "Device Organization");
        }

        var memberOrganizationSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(1));
        var memberOrganizationSeatId = memberOrganizationSeat.SeatId;

        var sharedInstallation = CreateInstallation(1);
        var personalShared = await ActivateAsync(
            database,
            member.UserId,
            member.SeatId,
            sharedInstallation,
            SignupTime.AddDays(2));
        var organizationShared = await ActivateAsync(
            database,
            member.UserId,
            memberOrganizationSeatId,
            sharedInstallation,
            SignupTime.AddDays(2));
        var repeatedPersonal = await ActivateAsync(
            database,
            member.UserId,
            member.SeatId,
            sharedInstallation,
            SignupTime.AddDays(2));
        var repeatedOrganization = await ActivateAsync(
            database,
            member.UserId,
            memberOrganizationSeatId,
            sharedInstallation,
            SignupTime.AddDays(2));
        Assert.Multiple(() =>
        {
            Assert.That(personalShared.ActivationId, Is.Not.EqualTo(organizationShared.ActivationId));
            Assert.That(repeatedPersonal.Status, Is.EqualTo(DeviceActivationStatus.AlreadyActive));
            Assert.That(repeatedPersonal.ActivationId, Is.EqualTo(personalShared.ActivationId));
            Assert.That(repeatedOrganization.Status, Is.EqualTo(DeviceActivationStatus.AlreadyActive));
            Assert.That(repeatedOrganization.ActivationId, Is.EqualTo(organizationShared.ActivationId));
        });

        for (var index = 2; index <= 3; index++)
        {
            await ActivateAsync(database, member.UserId, member.SeatId, index, SignupTime.AddDays(2));
            await ActivateAsync(
                database,
                member.UserId,
                memberOrganizationSeatId,
                index + 10,
                SignupTime.AddDays(2));
        }

        var personalRejected = await ActivateAsync(
            database,
            member.UserId,
            member.SeatId,
            4,
            SignupTime.AddDays(3));
        var organizationRejected = await ActivateAsync(
            database,
            member.UserId,
            memberOrganizationSeatId,
            14,
            SignupTime.AddDays(3));

        Assert.Multiple(() =>
        {
            Assert.That(personalRejected.Status, Is.EqualTo(DeviceActivationStatus.DeviceLimitReached));
            Assert.That(organizationRejected.Status, Is.EqualTo(DeviceActivationStatus.DeviceLimitReached));
        });
        await using var context = database.CreateContext();
        var activeCountsBySeat = await context.DeviceActivations
            .Where(activation => activation.UserId == member.UserId && activation.RevokedAt == null)
            .GroupBy(activation => activation.SeatId)
            .ToDictionaryAsync(group => group.Key, group => group.Count());
        Assert.Multiple(() =>
        {
            Assert.That(
                activeCountsBySeat.Keys,
                Is.EquivalentTo(new[] { member.SeatId, memberOrganizationSeatId }));
            Assert.That(activeCountsBySeat.Values, Has.All.EqualTo(3));
        });
    }

    [Test]
    public async Task DatabaseRequiresPositiveDeviceLimits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(
            async () => await context.Seats
                .Where(seat => seat.Id == signup.SeatId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    seat => seat.DeviceLimit,
                    0)));

        Assert.That(exception!.ConstraintName, Is.EqualTo("ck_seats_device_limit"));
    }

    [Test]
    public async Task ActivationRefusesToCompoundAnAlreadyReducedCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        for (var index = 1; index <= 3; index++)
        {
            await ActivateAsync(database, signup, index, SignupTime);
        }

        await using (var updateContext = database.CreateContext())
        {
            await updateContext.Seats
                .Where(seat => seat.Id == signup.SeatId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    seat => seat.DeviceLimit,
                    1));
        }

        var result = await ActivateAsync(database, signup, 4, SignupTime.AddDays(8));

        Assert.That(result.Status, Is.EqualTo(DeviceActivationStatus.DeviceLimitReached));
        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                context.DeviceActivations.Count(activation => activation.RevokedAt == null),
                Is.EqualTo(3));
            Assert.That(
                context.DeviceActivations.Count(activation => activation.RevokedAt != null),
                Is.Zero);
            Assert.That(context.AuditRecords.Count(), Is.EqualTo(3));
        });
    }

    [Test]
    public async Task DatabaseAllowsOnlyOneActiveInstallationPerSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var installation = CreateInstallation(1);
        await ActivateAsync(database, signup, installation, SignupTime);
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await InsertDeviceActivationAsync(
                context,
                signup.SeatId,
                signup.UserId,
                installation.InstallationId));

        Assert.That(exception!.InnerException, Is.TypeOf<Npgsql.PostgresException>());
        Assert.That(
            ((Npgsql.PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("ux_device_activations_active_seat_installation"));
    }

    [Test]
    public async Task DatabaseRequiresActivationUserToOwnTheSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "database-owner", "owner@example.com");
        var otherUser = await SignUpAsync(database, "database-other", "other@example.com");
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await InsertDeviceActivationAsync(
                context,
                owner.SeatId,
                otherUser.UserId,
                Guid.CreateVersion7()));

        Assert.That(exception!.InnerException, Is.TypeOf<Npgsql.PostgresException>());
        Assert.That(
            ((Npgsql.PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("fk_device_activations_seat_user"));
    }

    [TestCase(true)]
    [TestCase(false)]
    public async Task DatabaseRequiresRevocationTimeAndReasonTogether(bool onlySetReason)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var result = await ActivateAsync(database, signup, 1, SignupTime);
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(
            async () =>
            {
                if (onlySetReason)
                {
                    await context.DeviceActivations
                        .Where(activation => activation.Id == result.ActivationId)
                        .ExecuteUpdateAsync(setters => setters.SetProperty(
                            activation => activation.RevocationReason,
                            DeviceRevocationReason.Manual));
                }
                else
                {
                    await context.DeviceActivations
                        .Where(activation => activation.Id == result.ActivationId)
                        .ExecuteUpdateAsync(setters => setters.SetProperty(
                            activation => activation.RevokedAt,
                            SignupTime.AddDays(1)));
                }
            });

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_device_activations_timestamps"));
    }

    [Test]
    public async Task DatabaseRejectsRevocationBeforeLastSeen()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var result = await ActivateAsync(database, signup, 1, SignupTime);
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(
            async () => await context.DeviceActivations
                .Where(activation => activation.Id == result.ActivationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(activation => activation.LastSeenAt, SignupTime.AddDays(2))
                    .SetProperty(activation => activation.RevokedAt, SignupTime.AddDays(1))
                    .SetProperty(
                        activation => activation.RevocationReason,
                        DeviceRevocationReason.Manual)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_device_activations_timestamps"));
    }

    [Test]
    public async Task DatabaseRejectsDeviceActivityBeforeActivation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var result = await ActivateAsync(database, signup, 1, SignupTime);
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<Npgsql.PostgresException>(
            async () => await context.DeviceActivations
                .Where(activation => activation.Id == result.ActivationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    activation => activation.LastSeenAt,
                    SignupTime.AddSeconds(-1))));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_device_activations_timestamps"));
    }

    private static async Task<int> InsertDeviceActivationAsync(
        LicensingDbContext context,
        Guid seatId,
        Guid userId,
        Guid installationId)
    {
        var installation = DesktopInstallation.Create(
            installationId,
            "Direct database device",
            "linux",
            "x86_64",
            "1.2.3");
        context.DeviceActivations.Add(DeviceActivation.Activate(
            seatId,
            userId,
            installation,
            SignupTime));
        return await context.SaveChangesAsync();
    }


}
