using Microsoft.Extensions.DependencyInjection;
using Microsoft.EntityFrameworkCore;
using OpenIddict.Abstractions;
using OpenIddict.EntityFrameworkCore.Models;
using Styrhous.Licensing.Application.Desktop;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DesktopDeviceAuthorizationServiceTests
{
    private static readonly DateTimeOffset ObservedAt = SignupTime.AddDays(2);

    [Test]
    public async Task ApprovalAutomaticallySelectsTheOnlyEligiblePersonalSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var installation = CreateInstallation(1);
        await using var test = ServiceTestBase<DesktopDeviceAuthorizationService>.ForDatabase(database, ObservedAt);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var service = test.Service;

        var approval = await service.GetApprovalAsync(signup.UserId, installation);

        Assert.Multiple(() =>
        {
            Assert.That(
                approval.ReasonCode,
                Is.EqualTo(DesktopDeviceAuthorizationReasonCodes.AwaitingApproval));
            Assert.That(approval.Installation, Is.EqualTo(installation));
            Assert.That(approval.SelectedSeatId, Is.EqualTo(signup.SeatId));
            Assert.That(approval.EligibleSeats, Has.Count.EqualTo(1));
        });
        var seat = approval.EligibleSeats.Single();
        Assert.Multiple(() =>
        {
            Assert.That(seat.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(seat.BillingAccountId, Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(seat.Name, Is.EqualTo("Personal seat"));
            Assert.That(seat.EntitlementState, Is.EqualTo(EntitlementState.Trial));
            Assert.That(seat.EntitlementReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
            Assert.That(seat.DeviceLimit, Is.EqualTo(3));
            Assert.That(seat.CanActivate, Is.True);
            Assert.That(seat.ActiveDevices, Is.Empty);
        });
    }

    [Test]
    public async Task ApprovalUsesTheOrganizationNameForATransferredTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var organization = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Small Team",
            SignupTime.AddDays(1));
        await using var test = ServiceTestBase<DesktopDeviceAuthorizationService>.ForDatabase(database, ObservedAt);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var service = test.Service;

        var approval = await service.GetApprovalAsync(signup.UserId, CreateInstallation(1));

        Assert.That(approval.EligibleSeats, Has.Count.EqualTo(1));
        var seat = approval.EligibleSeats.Single();
        Assert.Multiple(() =>
        {
            Assert.That(seat.SeatId, Is.EqualTo(organization.SeatId));
            Assert.That(seat.BillingAccountId, Is.EqualTo(organization.BillingAccountId));
            Assert.That(seat.Name, Is.EqualTo("Small Team"));
            Assert.That(seat.EntitlementState, Is.EqualTo(EntitlementState.Trial));
            Assert.That(approval.SelectedSeatId, Is.EqualTo(organization.SeatId));
        });
    }

    [Test]
    public async Task ApprovalRequiresAChoiceWhenPersonalAndOrganizationSeatsAreEligible()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-multiple-seats",
            "multiple-seats@example.com");
        await AddActiveSubscriptionAsync(database, signup.PersonalBillingAccountId);

        var organization = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Two Seat Team",
            SignupTime.AddDays(1));
        await using var test = ServiceTestBase<DesktopDeviceAuthorizationService>.ForDatabase(database, ObservedAt);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var service = test.Service;

        var approval = await service.GetApprovalAsync(signup.UserId, CreateInstallation(1));

        Assert.Multiple(() =>
        {
            Assert.That(approval.SelectedSeatId, Is.Null);
            Assert.That(approval.EligibleSeats, Has.Count.EqualTo(2));
            Assert.That(
                approval.EligibleSeats.Select(seat => seat.SeatId),
                Does.Contain(signup.SeatId).And.Contain(organization.SeatId));
            Assert.That(
                approval.EligibleSeats.Select(seat => seat.Name),
                Does.Contain("Personal seat").And.Contain("Two Seat Team"));
        });
    }

    [Test]
    public async Task ApprovalReadUsesABoundedQuerySetAcrossManyEligibleSeats()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "desktop-batched-seats",
            "desktop-batched-seats@example.com");
        await AddActiveSubscriptionAsync(database, signup.PersonalBillingAccountId);
        for (var index = 1; index <= 6; index++)
        {
            var organization = await CreateOrganizationAsync(
                database,
                signup.UserId,
                $"Batched Team {index}",
                SignupTime.AddMinutes(index));
            await AddActiveSubscriptionAsync(database, organization.BillingAccountId);
        }

        var counter = new DatabaseCommandCounterInterceptor();
        await using var test = ServiceTestBase<DesktopDeviceAuthorizationService>.ForDatabase(database, ObservedAt, interceptors: [counter]);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var service = test.Service;
        var commandsBeforeApproval = counter.CommandCount;

        var approval = await service.GetApprovalAsync(
            signup.UserId,
            CreateInstallation(1));

        Assert.Multiple(() =>
        {
            Assert.That(approval.EligibleSeats, Has.Count.EqualTo(7));
            Assert.That(
                counter.CommandCount - commandsBeforeApproval,
                Is.LessThanOrEqualTo(6),
                "Approval reads must stay batched instead of querying once per seat.");
        });
    }

    [Test]
    public async Task ApprovalRequiresRevokingADeviceBeforeAFullSeatCanActivateAnother()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        await using var test = ServiceTestBase<DesktopDeviceAuthorizationService>.ForDatabase(database, ObservedAt);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var service = test.Service;
        var first = await ApproveAsync(
            context,
            service,
            signup.UserId,
            signup.SeatId,
            CreateInstallation(1));
        await ApproveAsync(
            context,
            service,
            signup.UserId,
            signup.SeatId,
            CreateInstallation(2));
        await ApproveAsync(
            context,
            service,
            signup.UserId,
            signup.SeatId,
            CreateInstallation(3));
        var fourthInstallation = CreateInstallation(4);

        var atCapacity = await ApproveAsync(
            context,
            service,
            signup.UserId,
            signup.SeatId,
            fourthInstallation);

        Assert.Multiple(() =>
        {
            Assert.That(
                atCapacity.Status,
                Is.EqualTo(DesktopDeviceAuthorizationDecisionStatus.DeviceLimitReached));
            Assert.That(
                atCapacity.ReasonCode,
                Is.EqualTo(DesktopDeviceAuthorizationReasonCodes.DeviceLimitReached));
            Assert.That(atCapacity.ActivationId, Is.Null);
            Assert.That(atCapacity.ActiveDevices, Has.Count.EqualTo(3));
        });

        await using var revocation = ServiceTestBase<DeviceRevocationService>.ForDatabase(database, ObservedAt.AddMinutes(1));
        var revoked = await revocation.Service
            .RevokeAsync(signup.UserId, first.ActivationId!.Value);
        var retried = await ApproveAsync(
            context,
            service,
            signup.UserId,
            signup.SeatId,
            fourthInstallation);

        Assert.Multiple(() =>
        {
            Assert.That(revoked.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
            Assert.That(
                retried.Status,
                Is.EqualTo(DesktopDeviceAuthorizationDecisionStatus.Approved));
            Assert.That(retried.ActivationId, Is.Not.Null);
            Assert.That(retried.ActiveDevices, Has.Count.EqualTo(3));
            Assert.That(
                retried.ActiveDevices.Select(device => device.InstallationId),
                Does.Contain(fourthInstallation.InstallationId));
            Assert.That(
                retried.ActiveDevices.Select(device => device.ActivationId),
                Does.Not.Contain(first.ActivationId));
        });
    }

    [Test]
    public async Task ReapprovingTheSameInstallationIsIdempotent()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var installation = CreateInstallation(1);
        await using var test = ServiceTestBase<DesktopDeviceAuthorizationService>.ForDatabase(database, ObservedAt);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var service = test.Service;

        var first = await ApproveAsync(
            context,
            service,
            signup.UserId,
            signup.SeatId,
            installation);
        var second = await ApproveAsync(
            context,
            service,
            signup.UserId,
            signup.SeatId,
            installation);

        Assert.Multiple(() =>
        {
            Assert.That(first.Status, Is.EqualTo(DesktopDeviceAuthorizationDecisionStatus.Approved));
            Assert.That(second.Status, Is.EqualTo(DesktopDeviceAuthorizationDecisionStatus.Approved));
            Assert.That(second.ActivationId, Is.EqualTo(first.ActivationId));
            Assert.That(second.ActiveDevices, Has.Count.EqualTo(1));
        });
    }

    [Test]
    public async Task ApprovalRejectsAnIneligibleSeatWithoutExposingItsDevices()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "desktop-owner", "owner@example.com");
        var other = await SignUpAsync(database, "desktop-other", "other@example.com");
        await using var test = ServiceTestBase<DesktopDeviceAuthorizationService>.ForDatabase(database, ObservedAt);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var service = test.Service;

        var result = await ApproveAsync(
            context,
            service,
            other.UserId,
            owner.SeatId,
            CreateInstallation(1));

        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(DesktopDeviceAuthorizationDecisionStatus.SeatNotEligible));
            Assert.That(
                result.ReasonCode,
                Is.EqualTo(DesktopDeviceAuthorizationReasonCodes.SeatNotEligible));
            Assert.That(result.ActivationId, Is.Null);
            Assert.That(result.ActiveDevices, Is.Empty);
        });
    }

    private static async Task<DesktopDeviceAuthorizationDecisionResult> ApproveAsync(
        LicensingDbContext context,
        DesktopDeviceAuthorizationService service,
        Guid userId,
        Guid seatId,
        DesktopInstallation installation)
    {
        var application = await context
            .Set<OpenIddictEntityFrameworkCoreApplication<Guid>>()
            .SingleAsync();
        var authorizationId = Guid.CreateVersion7();
        context.Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>().Add(
            new OpenIddictEntityFrameworkCoreAuthorization<Guid>
            {
                Id = authorizationId,
                Application = application,
                ConcurrencyToken = Guid.CreateVersion7().ToString(),
                CreationDate = ObservedAt.UtcDateTime,
                Status = OpenIddictConstants.Statuses.Valid,
                Subject = userId.ToString(),
                Type = OpenIddictConstants.AuthorizationTypes.AdHoc,
            });
        await context.SaveChangesAsync();
        return await service.ApproveAsync(
            userId,
            seatId,
            authorizationId,
            installation);
    }

    private static async Task AddActiveSubscriptionAsync(
        PostgresTestDatabase database,
        Guid billingAccountId)
    {
        var suffix = billingAccountId.ToString("N");
        await using var context = database.CreateContext();
        context.CommercialSubscriptions.Add(
            CommercialSubscription.Create(
                billingAccountId,
                new CommercialSubscriptionProjection(
                    $"cus_{suffix}",
                    $"sub_{suffix}",
                    "price_desktop",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime,
                    SignupTime.AddDays(30),
                    SignupTime)));
        await context.SaveChangesAsync();
    }
}
