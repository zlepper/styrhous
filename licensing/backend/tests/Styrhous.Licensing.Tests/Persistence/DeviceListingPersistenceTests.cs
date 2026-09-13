using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DeviceListingPersistenceTests
{
    [Test]
    public async Task ListsOnlyActiveDevicesInLastSeenOrderWithPersistedCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var oldest = await ActivateAsync(database, signup, 1, SignupTime);
        var newestInstallation = CreateInstallation(2);
        var newest = await ActivateAsync(
            database,
            signup,
            newestInstallation,
            SignupTime.AddDays(2));
        var middle = await ActivateAsync(database, signup, 3, SignupTime.AddDays(1));
        await using (var setupContext = database.CreateContext())
        {
            await setupContext.Seats
                .Where(seat => seat.Id == signup.SeatId)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(seat => seat.DeviceLimit, 5));
        }

        var result = await ListAsync(database, signup.UserId, signup.SeatId);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(DeviceListingStatus.Listed));
            Assert.That(result.ReasonCode, Is.EqualTo(DeviceListingReasonCodes.Listed));
            Assert.That(result.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(result.DeviceLimit, Is.EqualTo(5));
            Assert.That(
                result.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(
                    new[]
                    {
                        newest.ActivationId,
                        middle.ActivationId,
                        oldest.ActivationId,
                    }));
            Assert.That(result.ActiveDevices[0], Is.EqualTo(new ActiveDevice(
                newest.ActivationId!.Value,
                newestInstallation.InstallationId,
                "Device 2",
                "linux",
                "x86_64",
                "1.2.3",
                SignupTime.AddDays(2),
                SignupTime.AddDays(2))));
        });
    }

    [Test]
    public async Task CapacityAndDevicesComeFromOneRepeatableReadSnapshot()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var existing = await ActivateAsync(database, signup, 1, SignupTime);
        var addedInstallation = CreateInstallation(2);
        var addedActivationId = Guid.CreateVersion7();
        var gate = new DatabaseCommandGate();
        await using var listingTest = ServiceTestBase<DeviceListingService>.ForDatabase(
            database, SignupTime, interceptors: [
                    new DatabaseCommandGateInterceptor(gate, "FROM device_activations")]);
        var listingService = listingTest.Service;

        var listingTask = listingService.ListActiveAsync(signup.UserId, signup.SeatId);
        await gate.WaitUntilReachedAsync();
        try
        {
            await using var updateContext = database.CreateContext();
            await using var updateTransaction =
                await updateContext.Database.BeginTransactionAsync();
            await updateContext.Seats
                .Where(seat => seat.Id == signup.SeatId)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(seat => seat.DeviceLimit, 5));
            var addedActivation = DeviceActivation.Activate(
                signup.SeatId,
                signup.UserId,
                addedInstallation,
                SignupTime.AddDays(1));
            updateContext.Entry(addedActivation)
                .Property(activation => activation.Id)
                .CurrentValue = addedActivationId;
            updateContext.DeviceActivations.Add(addedActivation);
            await updateContext.SaveChangesAsync();
            await updateTransaction.CommitAsync();
        }
        finally
        {
            gate.Release();
        }

        var duringWrite = await listingTask;
        var afterWrite = await ListAsync(database, signup.UserId, signup.SeatId);

        Assert.Multiple(() =>
        {
            Assert.That(duringWrite.DeviceLimit, Is.EqualTo(3));
            Assert.That(
                duringWrite.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { existing.ActivationId }));
            Assert.That(afterWrite.DeviceLimit, Is.EqualTo(5));
            Assert.That(
                afterWrite.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { addedActivationId, existing.ActivationId!.Value }));
        });
    }

    [Test]
    public async Task SameUsersSeatsKeepCapacityAndDevicesIsolatedAfterTrialTransfer()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var personalActivation = await ActivateAsync(database, signup, 1, SignupTime);
        OrganizationCreationResult organization;
        await using (var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
                database, SignupTime.AddDays(1)))
        {
            organization = await organizationTest.Service
                .CreateAsync(signup.UserId, "Listing Organization");
        }

        var organizationActivation = await ActivateAsync(
            database,
            signup.UserId,
            organization.SeatId,
            2,
            SignupTime.AddDays(2));
        await using (var setupContext = database.CreateContext())
        {
            await setupContext.Seats
                .Where(seat => seat.Id == organization.SeatId)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(seat => seat.DeviceLimit, 5));
        }

        var personal = await ListAsync(database, signup.UserId, signup.SeatId);
        var organizationResult = await ListAsync(
            database,
            signup.UserId,
            organization.SeatId);

        Assert.Multiple(() =>
        {
            Assert.That(personal.DeviceLimit, Is.EqualTo(3));
            Assert.That(
                personal.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { personalActivation.ActivationId }));
            Assert.That(organizationResult.DeviceLimit, Is.EqualTo(5));
            Assert.That(
                organizationResult.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { organizationActivation.ActivationId }));
        });
    }

    [Test]
    public async Task EqualLastSeenTimesAreOrderedByAscendingActivationId()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activations = new[]
        {
            await ActivateAsync(database, signup, 1, SignupTime),
            await ActivateAsync(database, signup, 2, SignupTime),
            await ActivateAsync(database, signup, 3, SignupTime),
        };

        var result = await ListAsync(database, signup.UserId, signup.SeatId);

        Assert.That(
            result.ActiveDevices.Select(device => device.ActivationId),
            Is.EqualTo(activations.Select(activation => activation.ActivationId).Order()));
    }

    [Test]
    public async Task SuccessfulEntitlementCheckMovesDeviceToMostRecentPosition()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var first = await ActivateAsync(database, signup, 1, SignupTime);
        var second = await ActivateAsync(
            database,
            signup,
            2,
            SignupTime.AddMinutes(30));

        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddHours(1));
        await checkTest.Service
            .CheckAsync(signup.UserId, first.ActivationId!.Value);
        var result = await ListAsync(database, signup.UserId, signup.SeatId);

        Assert.That(
            result.ActiveDevices.Select(device => device.ActivationId),
            Is.EqualTo(new[] { first.ActivationId, second.ActivationId }));
        Assert.That(result.ActiveDevices[0].LastSeenAt, Is.EqualTo(SignupTime.AddHours(1)));
    }

    [Test]
    public async Task RevokedDevicesAreExcludedAndFreshChangesAppearThroughAReusedStore()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var first = await ActivateAsync(database, signup, 1, SignupTime);
        await using var listingTest = ServiceTestBase<DeviceListingService>.ForDatabase(
            database, SignupTime);
        var service = listingTest.Service;

        var initial = await service.ListActiveAsync(signup.UserId, signup.SeatId);
        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(1));
        await revocationTest.Service
            .RevokeAsync(signup.UserId, first.ActivationId!.Value);
        var afterRevocation = await service.ListActiveAsync(signup.UserId, signup.SeatId);
        var replacement = await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        var afterReplacement = await service.ListActiveAsync(signup.UserId, signup.SeatId);

        Assert.Multiple(() =>
        {
            Assert.That(initial.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { first.ActivationId }));
            Assert.That(afterRevocation.Status, Is.EqualTo(DeviceListingStatus.Listed));
            Assert.That(afterRevocation.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(afterRevocation.DeviceLimit, Is.EqualTo(3));
            Assert.That(afterRevocation.ActiveDevices, Is.Empty);
            Assert.That(afterReplacement.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { replacement.ActivationId }));
        });
    }

    [Test]
    public async Task ListingDoesNotRequireACurrentEntitlement()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        await using (var setupContext = database.CreateContext())
        {
            await setupContext.Trials.ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(trial => trial.StartedAt, SignupTime.AddDays(-31))
                    .SetProperty(trial => trial.EndsAt, SignupTime.AddDays(-1)));
        }

        var result = await ListAsync(database, signup.UserId, signup.SeatId);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(DeviceListingStatus.Listed));
            Assert.That(
                result.ActiveDevices.Select(device => device.ActivationId),
                Is.EqualTo(new[] { activation.ActivationId }));
        });
    }

    [Test]
    public async Task UnknownAndOtherUsersSeatsShareTheNotFoundResult()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "list-owner", "owner@example.com");
        var other = await SignUpAsync(database, "list-other", "other@example.com");
        await ActivateAsync(database, owner, 1, SignupTime);

        var otherUser = await ListAsync(database, other.UserId, owner.SeatId);
        var unknown = await ListAsync(database, owner.UserId, Guid.CreateVersion7());

        foreach (var result in new[] { otherUser, unknown })
        {
            Assert.Multiple(() =>
            {
                Assert.That(result.Status, Is.EqualTo(DeviceListingStatus.SeatNotFound));
                Assert.That(
                    result.ReasonCode,
                    Is.EqualTo(DeviceListingReasonCodes.SeatNotFound));
                Assert.That(result.SeatId, Is.Null);
                Assert.That(result.DeviceLimit, Is.Null);
                Assert.That(result.ActiveDevices, Is.Empty);
            });
        }
    }

    [TestCase("user")]
    [TestCase("seat")]
    public async Task MissingOwnedIdentifiersDoNotRevealOrChangeDevices(string emptyIdentifier)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await LicensingPersistenceScenario.SignUpAsync(database);
        await DevicePersistenceScenario.ActivateAsync(database, owner, 1, SignupTime);
        await using var listingTest = ServiceTestBase<DeviceListingService>.ForDatabase(
            database, SignupTime);
        var service = listingTest.Service;

        var result = await service.ListActiveAsync(
            emptyIdentifier == "user" ? Guid.Empty : owner.UserId,
            emptyIdentifier == "seat" ? Guid.Empty : owner.SeatId);
        Assert.That(result.Status, Is.EqualTo(DeviceListingStatus.SeatNotFound));
        await using var verification = database.CreateContext();
        Assert.That((await verification.DeviceActivations.SingleAsync()).RevokedAt, Is.Null);
    }

    private static async Task<DeviceListingResult> ListAsync(
        PostgresTestDatabase database,
        Guid userId,
        Guid seatId)
    {
        await using var listingTest = ServiceTestBase<DeviceListingService>.ForDatabase(
            database, SignupTime);
        return await listingTest.Service
            .ListActiveAsync(userId, seatId);
    }
}
