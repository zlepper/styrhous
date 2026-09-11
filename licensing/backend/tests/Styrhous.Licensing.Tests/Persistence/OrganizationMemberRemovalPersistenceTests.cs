using Microsoft.EntityFrameworkCore;
using OpenIddict.Abstractions;
using OpenIddict.EntityFrameworkCore.Models;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationMemberRemovalPersistenceTests
{
    [Test]
    public async Task OwnerRemovalDeletesMembershipSeatAndDeviceHistoryButRetainsAuditHistory()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "remove-owner", "owner@example.com");
        var member = await SignUpAsync(database, "remove-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var firstActivation = await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var secondActivation = await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            2,
            SignupTime.AddDays(2));
        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, SignupTime.AddDays(2).AddHours(1));
        var manualRevocation = await revocationTest.Service
            .RevokeAsync(member.UserId, firstActivation.ActivationId!.Value);
        var removedAt = SignupTime.AddDays(3).ToOffset(TimeSpan.FromHours(2));
        await using var context = database.CreateContext();
        var application = await context
            .Set<OpenIddictEntityFrameworkCoreApplication<Guid>>()
            .SingleAsync();
        var authorization = new OpenIddictEntityFrameworkCoreAuthorization<Guid>
        {
            Id = Guid.CreateVersion7(),
            Application = application,
            ConcurrencyToken = Guid.CreateVersion7().ToString(),
            CreationDate = SignupTime.AddDays(2).UtcDateTime,
            Status = OpenIddictConstants.Statuses.Valid,
            Subject = member.UserId.ToString(),
            Type = OpenIddictConstants.AuthorizationTypes.AdHoc,
        };
        var refreshToken = new OpenIddictEntityFrameworkCoreToken<Guid>
        {
            Id = Guid.CreateVersion7(),
            Application = application,
            Authorization = authorization,
            ConcurrencyToken = Guid.CreateVersion7().ToString(),
            CreationDate = SignupTime.AddDays(2).UtcDateTime,
            ExpirationDate = SignupTime.AddDays(92).UtcDateTime,
            Status = OpenIddictConstants.Statuses.Valid,
            Subject = member.UserId.ToString(),
            Type = OpenIddictConstants.TokenTypeIdentifiers.RefreshToken,
        };
        context.AddRange(
            authorization,
            refreshToken,
            DesktopDeviceSession.Start(
                secondActivation.ActivationId!.Value,
                authorization.Id,
                SignupTime.AddDays(2)));
        await context.SaveChangesAsync();

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, removedAt);
        var result = await removalTest.Service.RemoveAsync(
            owner.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId);
        var success = RequireSuccess(result);

        Assert.Multiple(() =>
        {
            Assert.That(success.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(success.MembershipId, Is.EqualTo(memberSetup.MembershipId));
            Assert.That(success.UserId, Is.EqualTo(member.UserId));
            Assert.That(success.SeatId, Is.EqualTo(memberSetup.SeatId));
            Assert.That(success.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(success.RemovedAt, Is.EqualTo(removedAt.ToUniversalTime()));
            Assert.That(success.RemovedAt.Offset, Is.EqualTo(TimeSpan.Zero));
            Assert.That(manualRevocation.Status, Is.EqualTo(DeviceRevocationStatus.Revoked));
        });

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Any(
                    membership => membership.Id == memberSetup.MembershipId),
                Is.False);
            Assert.That(verificationContext.Seats.Any(seat => seat.Id == memberSetup.SeatId), Is.False);
            Assert.That(
                verificationContext.DeviceActivations.Any(
                    activation => activation.SeatId == memberSetup.SeatId),
                Is.False);
            Assert.That(
                verificationContext.Seats.Count(seat => seat.AssignedUserId == member.UserId),
                Is.EqualTo(1),
                "The removed member's personal seat must remain available.");
            Assert.That(verificationContext.UserAccounts.Any(user => user.Id == member.UserId), Is.True);
            Assert.That(
                verificationContext.DesktopDeviceSessions.Any(session =>
                    session.AuthorizationId == authorization.Id),
                Is.False);
            Assert.That(
                verificationContext.Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
                    .Single(candidate => candidate.Id == authorization.Id)
                    .Status,
                Is.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(
                verificationContext.Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
                    .Single(token => token.Id == refreshToken.Id)
                    .Status,
                Is.EqualTo(OpenIddictConstants.Statuses.Revoked));
        });

        var removalAudits = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == success.CorrelationId)
            .OrderBy(record => record.Action)
            .ToArrayAsync();
        var removedActivationIds = new[]
        {
            firstActivation.ActivationId!.Value,
            secondActivation.ActivationId!.Value,
        };
        Assert.Multiple(() =>
        {
            Assert.That(removalAudits, Has.Length.EqualTo(4));
            Assert.That(removalAudits.All(record => record.Id.Version == 7), Is.True);
            Assert.That(removalAudits.All(record => record.ActorUserId == owner.UserId), Is.True);
            Assert.That(removalAudits.All(record => record.OccurredAt == removedAt.ToUniversalTime()),
                Is.True);
            Assert.That(
                removalAudits
                    .Where(record => record.Action == AuditAction.DeviceActivationRemoved)
                    .Select(record => record.TargetId),
                Is.EquivalentTo(removedActivationIds));
            Assert.That(
                removalAudits
                    .Where(record => record.Action == AuditAction.DeviceActivationRemoved)
                    .All(record => record.TargetType == AuditTargetType.DeviceActivation),
                Is.True);
            Assert.That(
                removalAudits.Single(record => record.Action == AuditAction.SeatUnassigned)
                    .TargetId,
                Is.EqualTo(memberSetup.SeatId));
            Assert.That(
                removalAudits.Single(record => record.Action == AuditAction.SeatUnassigned)
                    .TargetType,
                Is.EqualTo(AuditTargetType.Seat));
            Assert.That(
                removalAudits.Single(
                        record => record.Action == AuditAction.OrganizationMemberRemoved)
                    .TargetId,
                Is.EqualTo(memberSetup.MembershipId));
            Assert.That(
                removalAudits.Single(
                        record => record.Action == AuditAction.OrganizationMemberRemoved)
                    .TargetType,
                Is.EqualTo(AuditTargetType.OrganizationMembership));
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.Action == AuditAction.DeviceActivated
                    && removedActivationIds.Contains(record.TargetId)),
                Is.EqualTo(2),
                "Existing immutable activation audit records must outlive deleted device rows.");
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.Action == AuditAction.DeviceRevokedManual
                    && record.TargetId == firstActivation.ActivationId),
                Is.EqualTo(1),
                "Existing immutable revocation history must outlive the deleted device row.");
        });
    }

    [Test]
    public async Task RemovingSeatlessMemberDeletesRetainedDeviceSessionWithoutUnassigningAgain()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "remove-seatless-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "remove-seatless-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seatless Removal Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var activation = await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var removedAt = SignupTime.AddDays(3);
        await using var context = database.CreateContext();
        var application = await context
            .Set<OpenIddictEntityFrameworkCoreApplication<Guid>>()
            .SingleAsync();
        var authorization = new OpenIddictEntityFrameworkCoreAuthorization<Guid>
        {
            Id = Guid.CreateVersion7(),
            Application = application,
            ConcurrencyToken = Guid.CreateVersion7().ToString(),
            CreationDate = SignupTime.AddDays(2).UtcDateTime,
            Status = OpenIddictConstants.Statuses.Valid,
            Subject = member.UserId.ToString(),
            Type = OpenIddictConstants.AuthorizationTypes.AdHoc,
        };
        var session = DesktopDeviceSession.Start(
            activation.ActivationId!.Value,
            authorization.Id,
            SignupTime.AddDays(2));
        context.AddRange(authorization, session);
        await context.SaveChangesAsync();
        await context.Seats
            .Where(seat => seat.Id == memberSetup.SeatId)
            .ExecuteUpdateAsync(setters => setters.SetProperty(
                seat => seat.ProductAccessEnabled,
                false));

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, removedAt);
        var result = await removalTest.Service.RemoveAsync(
            owner.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId);
        var success = RequireSuccess(result);

        await using var verificationContext = database.CreateContext();
        var removalAudits = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == success.CorrelationId)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Any(
                    membership => membership.Id == memberSetup.MembershipId),
                Is.False);
            Assert.That(verificationContext.Seats.Any(seat => seat.Id == memberSetup.SeatId), Is.False);
            Assert.That(
                verificationContext.DeviceActivations.Any(device => device.Id == activation.ActivationId),
                Is.False);
            Assert.That(
                verificationContext.DesktopDeviceSessions.Any(candidate => candidate.Id == session.Id),
                Is.False);
            Assert.That(
                verificationContext.Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
                    .Single(candidate => candidate.Id == authorization.Id)
                    .Status,
                Is.EqualTo(OpenIddictConstants.Statuses.Revoked));
            Assert.That(
                removalAudits.Count(record => record.Action == AuditAction.SeatUnassigned),
                Is.Zero);
            Assert.That(
                removalAudits.Count(record =>
                    record.Action == AuditAction.DeviceActivationRemoved),
                Is.EqualTo(1));
            Assert.That(
                removalAudits.Count(record =>
                    record.Action == AuditAction.OrganizationMemberRemoved),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task RemovingOneMembershipPreservesOtherOrganizationAccessForTheSameUser()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var firstOwner = await SignUpAsync(
            database,
            "multi-org-first-owner",
            "first-owner@example.com");
        var secondOwner = await SignUpAsync(
            database,
            "multi-org-second-owner",
            "second-owner@example.com");
        var member = await SignUpAsync(
            database,
            "multi-org-member",
            "member@example.com");
        var firstOrganization = await CreateOrganizationAsync(
            database,
            firstOwner.UserId,
            "First Membership Organization",
            SignupTime.AddDays(1));
        var secondOrganization = await CreateOrganizationAsync(
            database,
            secondOwner.UserId,
            "Second Membership Organization",
            SignupTime.AddDays(1));
        var firstMembership = await AddOrganizationMemberAsync(
            database,
            firstOrganization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        var secondMembership = await AddOrganizationMemberAsync(
            database,
            secondOrganization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        var firstActivation = await ActivateAsync(
            database,
            member.UserId,
            firstMembership.SeatId,
            1,
            SignupTime.AddDays(3));
        var secondActivation = await ActivateAsync(
            database,
            member.UserId,
            secondMembership.SeatId,
            2,
            SignupTime.AddDays(3));

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(4));
        var removal = await removalTest.Service.RemoveAsync(
            firstOwner.UserId,
            firstOrganization.OrganizationId,
            firstMembership.MembershipId);

        Assert.That(removal.Status, Is.EqualTo(OrganizationMemberRemovalStatus.Removed));

        await using var verificationContext = database.CreateContext();
        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(4));
        var entitlements = await entitlementTest.Service
            .ListForUserAsync(member.UserId);
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Any(membership =>
                    membership.Id == firstMembership.MembershipId),
                Is.False);
            Assert.That(verificationContext.Seats.Any(seat => seat.Id == firstMembership.SeatId), Is.False);
            Assert.That(
                verificationContext.DeviceActivations.Any(activation =>
                    activation.Id == firstActivation.ActivationId),
                Is.False);
            Assert.That(
                verificationContext.OrganizationMemberships.Any(membership =>
                    membership.Id == secondMembership.MembershipId
                    && membership.OrganizationId == secondOrganization.OrganizationId),
                Is.True);
            Assert.That(verificationContext.Seats.Any(seat => seat.Id == secondMembership.SeatId), Is.True);
            Assert.That(
                verificationContext.DeviceActivations.Any(activation =>
                    activation.Id == secondActivation.ActivationId),
                Is.True);
            Assert.That(
                entitlements.Single(entitlement =>
                    entitlement.SeatId == secondMembership.SeatId).IsEligible,
                Is.True);
            Assert.That(
                verificationContext.Seats.Count(seat => seat.AssignedUserId == member.UserId),
                Is.EqualTo(2),
                "The personal and second-organization seats must remain.");
            Assert.That(verificationContext.UserAccounts.Any(user => user.Id == member.UserId), Is.True);
        });
    }

    [TestCase(OrganizationRole.Owner, OrganizationRole.Admin, false)]
    [TestCase(OrganizationRole.Owner, OrganizationRole.Member, false)]
    [TestCase(OrganizationRole.Admin, OrganizationRole.Member, false)]
    [TestCase(OrganizationRole.Admin, OrganizationRole.Admin, true)]
    [TestCase(OrganizationRole.Member, OrganizationRole.Member, true)]
    public async Task AuthorizedActorsCanRemoveOrLeave(
        OrganizationRole actorRole,
        OrganizationRole targetRole,
        bool selfRemoval)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"authorized-owner-{actorRole}-{targetRole}-{selfRemoval}",
            $"owner-{actorRole}-{targetRole}-{selfRemoval}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Authorized Removal Organization",
            SignupTime.AddDays(1));

        Guid actorUserId;
        Guid targetMembershipId;
        if (actorRole == OrganizationRole.Owner)
        {
            actorUserId = owner.UserId;
            var target = await SignUpAsync(
                database,
                $"authorized-target-{targetRole}-{selfRemoval}",
                $"target-{targetRole}-{selfRemoval}@example.com");
            targetMembershipId = (await AddOrganizationMemberAsync(
                    database,
                    organization,
                    target.UserId,
                    targetRole))
                .MembershipId;
        }
        else
        {
            var actor = await SignUpAsync(
                database,
                $"authorized-actor-{actorRole}-{targetRole}-{selfRemoval}",
                $"actor-{actorRole}-{targetRole}-{selfRemoval}@example.com");
            var actorMembership = await AddOrganizationMemberAsync(
                database,
                organization,
                actor.UserId,
                actorRole);
            actorUserId = actor.UserId;
            if (selfRemoval)
            {
                targetMembershipId = actorMembership.MembershipId;
            }
            else
            {
                var target = await SignUpAsync(
                    database,
                    $"authorized-target-{actorRole}-{targetRole}",
                    $"target-{actorRole}-{targetRole}@example.com");
                targetMembershipId = (await AddOrganizationMemberAsync(
                        database,
                        organization,
                        target.UserId,
                        targetRole))
                    .MembershipId;
            }
        }

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var result = await removalTest.Service.RemoveAsync(
            actorUserId,
            organization.OrganizationId,
            targetMembershipId);

        Assert.That(result.Status, Is.EqualTo(OrganizationMemberRemovalStatus.Removed));

        await using var verificationContext = database.CreateContext();
        Assert.That(
            await verificationContext.OrganizationMemberships.AnyAsync(
                membership => membership.Id == targetMembershipId),
            Is.False);
    }

    [TestCase(OrganizationRole.Admin, OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member, OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member, OrganizationRole.Member)]
    public async Task UnauthorizedActorCannotRemoveAnotherMember(
        OrganizationRole actorRole,
        OrganizationRole targetRole)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"denied-owner-{actorRole}-{targetRole}",
            $"owner-{actorRole}-{targetRole}@example.com");
        var actor = await SignUpAsync(
            database,
            $"denied-actor-{actorRole}-{targetRole}",
            $"actor-{actorRole}-{targetRole}@example.com");
        var target = await SignUpAsync(
            database,
            $"denied-target-{actorRole}-{targetRole}",
            $"target-{actorRole}-{targetRole}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Denied Removal Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, actor.UserId, actorRole);
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            targetRole);
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var result = await removalTest.Service.RemoveAsync(
            actor.UserId,
            organization.OrganizationId,
            targetSetup.MembershipId);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationMemberRemovalStatus.InsufficientPermission));
        Assert.Multiple(() =>
        {
            Assert.That(
                context.OrganizationMemberships.Any(
                    membership => membership.Id == targetSetup.MembershipId),
                Is.True);
            Assert.That(context.Seats.Any(seat => seat.Id == targetSetup.SeatId), Is.True);
            Assert.That(context.AuditRecords.Count(), Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task OwnerCannotBeRemovedOrLeaveBeforeOwnershipTransfer()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "owner-guard-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "owner-guard-admin", "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Owner Guard Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        await using var context = database.CreateContext();
        var ownerMembership = await context.OrganizationMemberships.SingleAsync(
            membership => membership.UserId == owner.UserId
                && membership.OrganizationId == organization.OrganizationId);

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var selfResult = await removalTest.Service.RemoveAsync(
            owner.UserId,
            organization.OrganizationId,
            ownerMembership.Id);
        await using var removalTest2 = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var adminResult = await removalTest2.Service.RemoveAsync(
            admin.UserId,
            organization.OrganizationId,
            ownerMembership.Id);

        Assert.Multiple(() =>
        {
            Assert.That(selfResult.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.OwnershipTransferRequired));
            Assert.That(adminResult.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.OwnershipTransferRequired));
            Assert.That(
                context.OrganizationMemberships.Count(membership =>
                    membership.Role == OrganizationRole.Owner),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task OutsiderCannotDiscoverOrganizationAndMemberIdentifiersAreOrganizationScoped()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "private-owner", "owner@example.com");
        var outsider = await SignUpAsync(database, "private-outsider", "outsider@example.com");
        var member = await SignUpAsync(database, "private-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Private Removal Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Other Removal Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await using var context = database.CreateContext();
        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var service = removalTest.Service;

        var outsiderResult = await service.RemoveAsync(
            outsider.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId);
        var unknownOrganizationResult = await service.RemoveAsync(
            owner.UserId,
            Guid.CreateVersion7(),
            memberSetup.MembershipId);
        var crossOrganizationResult = await service.RemoveAsync(
            owner.UserId,
            otherOrganization.OrganizationId,
            memberSetup.MembershipId);
        var unknownMemberResult = await service.RemoveAsync(
            owner.UserId,
            organization.OrganizationId,
            Guid.CreateVersion7());

        Assert.Multiple(() =>
        {
            Assert.That(outsiderResult.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.OrganizationNotFound));
            Assert.That(unknownOrganizationResult.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.OrganizationNotFound));
            Assert.That(crossOrganizationResult.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.MemberNotFound));
            Assert.That(unknownMemberResult.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.MemberNotFound));
            Assert.That(
                context.OrganizationMemberships.Any(
                    membership => membership.Id == memberSetup.MembershipId),
                Is.True);
        });
    }

    [Test]
    public async Task RemovingMemberImmediatelyFreesReservedTrialCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "capacity-owner", "owner@example.com");
        var member = await SignUpAsync(database, "capacity-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Capacity Removal Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        for (var index = 0; index < 3; index++)
        {
            await CreateInvitationAsync(
                database,
                owner.UserId,
                organization.OrganizationId,
                $"pending-{index}@example.com",
                SignupTime.AddDays(2));
        }

        await using var invitationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var invitationService = invitationTest.Service;
        var beforeRemoval = await invitationService.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "replacement@example.com",
            OrganizationRole.Member);
        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var removal = await removalTest.Service.RemoveAsync(
            owner.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId);
        var afterRemoval = await invitationService.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "replacement@example.com",
            OrganizationRole.Member);

        Assert.Multiple(() =>
        {
            Assert.That(beforeRemoval.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
            Assert.That(removal.Status, Is.EqualTo(OrganizationMemberRemovalStatus.Removed));
            Assert.That(afterRemoval.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
        });
    }

    [Test]
    public async Task ConcurrentRepeatedRemovalDeletesAndAuditsExactlyOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "race-owner", "owner@example.com");
        var member = await SignUpAsync(database, "race-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Race Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(3), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "FROM user_accounts")]);
        await using var removalTest2 = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(3), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "FROM user_accounts")]);
        var results = await Task.WhenAll(
            removalTest.Service.RemoveAsync(
                owner.UserId,
                organization.OrganizationId,
                memberSetup.MembershipId),
            removalTest2.Service.RemoveAsync(
                owner.UserId,
                organization.OrganizationId,
                memberSetup.MembershipId));

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result => result.Status == OrganizationMemberRemovalStatus.Removed),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationMemberRemovalStatus.MemberNotFound),
                Is.EqualTo(1));
        });
        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.Action == AuditAction.OrganizationMemberRemoved),
                Is.EqualTo(1));
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.Action == AuditAction.SeatUnassigned),
                Is.EqualTo(1));
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.Action == AuditAction.DeviceActivationRemoved),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ConcurrentActivationCannotSurviveMembershipRemoval()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "activation-race-owner", "owner@example.com");
        var member = await SignUpAsync(database, "activation-race-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Activation Removal Race Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var activationFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var removalTask = removalTest.Service
            .RemoveAsync(
                owner.UserId,
                organization.OrganizationId,
                memberSetup.MembershipId);
        var activationTask = DevicePersistenceScenario.ActivateAsync(activationFactory, SignupTime.AddDays(2),
                member.UserId,
                memberSetup.SeatId,
                CreateInstallation(1));
        await Task.WhenAll(removalTask, activationTask);

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(removalTask.Result.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.Removed));
            Assert.That(
                activationTask.Result.Status,
                Is.AnyOf(DeviceActivationStatus.Activated, DeviceActivationStatus.SeatNotEligible));
        });
        await using var verificationContext = database.CreateContext();
        var removedActivationAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.DeviceActivationRemoved);
        var memberConcurrencyVersion = await verificationContext.UserAccounts
            .Where(user => user.Id == member.UserId)
            .Select(user => user.ConcurrencyVersion)
            .SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Any(membership =>
                    membership.Id == memberSetup.MembershipId),
                Is.False);
            Assert.That(verificationContext.Seats.Any(seat => seat.Id == memberSetup.SeatId),
                Is.False);
            Assert.That(
                verificationContext.DeviceActivations.Any(activation =>
                    activation.SeatId == memberSetup.SeatId),
                Is.False);
            Assert.That(
                removedActivationAuditCount,
                Is.EqualTo(
                    activationTask.Result.Status == DeviceActivationStatus.Activated ? 1 : 0));
            Assert.That(
                memberConcurrencyVersion,
                Is.InRange(1, 2));
        });
    }

    [Test]
    public async Task RepeatedMembershipDeleteConflictsRollBackSeatDevicesAndAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "remove-conflict-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "remove-conflict-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Conflict Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var activation = await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var interceptor = new OrganizationMemberRemovalConflictInterceptor();
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(3), interceptors: [interceptor]);
        var result = await removalTest.Service.RemoveAsync(
            owner.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId);

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.ConcurrentModification));
            Assert.That(interceptor.SuppressedDeleteCount, Is.EqualTo(3));
            Assert.That(
                verificationContext.OrganizationMemberships.Any(membership =>
                    membership.Id == memberSetup.MembershipId),
                Is.True);
            Assert.That(
                verificationContext.Seats.Any(seat => seat.Id == memberSetup.SeatId),
                Is.True);
            Assert.That(
                verificationContext.DeviceActivations.Any(device =>
                    device.Id == activation.ActivationId
                    && device.SeatId == memberSetup.SeatId),
                Is.True);
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task RepeatedSerializationFailuresReturnAStableConflictWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "remove-serialization-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "remove-serialization-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Serialization Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var activation = await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var interceptor = new OrganizationMemberRemovalSerializationFailureInterceptor();
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(3), interceptors: [interceptor]);
        var result = await removalTest.Service.RemoveAsync(
            owner.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId);

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.ConcurrentModification));
            Assert.That(interceptor.FailureCount, Is.EqualTo(3));
            Assert.That(
                verificationContext.OrganizationMemberships.Any(membership =>
                    membership.Id == memberSetup.MembershipId),
                Is.True);
            Assert.That(
                verificationContext.Seats.Any(seat => seat.Id == memberSetup.SeatId),
                Is.True);
            Assert.That(
                verificationContext.DeviceActivations.Any(device =>
                    device.Id == activation.ActivationId
                    && device.SeatId == memberSetup.SeatId),
                Is.True);
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task FailureAfterSavingRollsBackAllDeletesAndRemovalAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "rollback-owner", "owner@example.com");
        var member = await SignUpAsync(database, "rollback-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Rollback Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime.AddDays(3), interceptors: [new ThrowAfterSaveInterceptor()]);
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await removalTest.Service.RemoveAsync(
                owner.UserId,
                organization.OrganizationId,
                memberSetup.MembershipId));

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Any(membership =>
                    membership.Id == memberSetup.MembershipId),
                Is.True);
            Assert.That(verificationContext.Seats.Any(seat => seat.Id == memberSetup.SeatId),
                Is.True);
            Assert.That(
                verificationContext.DeviceActivations.Any(activation =>
                    activation.SeatId == memberSetup.SeatId),
                Is.True);
            Assert.That(verificationContext.AuditRecords.Count(), Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task UnknownActorCannotRemoveMember()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await removalTest.Service.RemoveAsync(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                Guid.CreateVersion7()));
        Assert.That(await context.OrganizationMemberships.CountAsync(), Is.Zero);
        Assert.That(await context.Seats.CountAsync(), Is.Zero);
        Assert.That(await context.DeviceActivations.CountAsync(), Is.Zero);
        Assert.That(await context.AuditRecords.CountAsync(), Is.Zero);
    }

    private static OrganizationMemberRemovalResult.Success RequireSuccess(
        OrganizationMemberRemovalResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationMemberRemovalResult.Success>());
        return (OrganizationMemberRemovalResult.Success)result;
    }
}
