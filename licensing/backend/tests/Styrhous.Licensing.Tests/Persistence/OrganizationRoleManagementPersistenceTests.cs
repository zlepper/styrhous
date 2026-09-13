using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationRoleManagementPersistenceTests
{
    [TestCase(OrganizationRole.Member, OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Admin, OrganizationRole.Member)]
    public async Task OwnerChangesMemberRoleWithoutChangingTheirSeatOrDevices(
        OrganizationRole currentRole,
        OrganizationRole requestedRole)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"role-owner-{currentRole}",
            $"owner-{currentRole}@example.com");
        var member = await SignUpAsync(
            database,
            $"role-member-{currentRole}",
            $"member-{currentRole}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            currentRole);
        var activation = await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var observedAt = SignupTime.AddDays(3);
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, observedAt);
        var result = await roleTest.Service.ChangeRoleAsync(
            owner.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId,
            requestedRole);

        var success = RequireRoleChange(result);

        await using var verificationContext = database.CreateContext();
        var audit = await verificationContext.AuditRecords.SingleAsync(
            record => record.CorrelationId == success.CorrelationId);
        Assert.Multiple(() =>
        {
            Assert.That(success.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(success.MembershipId, Is.EqualTo(memberSetup.MembershipId));
            Assert.That(success.UserId, Is.EqualTo(member.UserId));
            Assert.That(success.PreviousRole, Is.EqualTo(currentRole));
            Assert.That(success.Role, Is.EqualTo(requestedRole));
            Assert.That(success.ChangedAt, Is.EqualTo(observedAt));
            Assert.That(success.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == memberSetup.MembershipId).Role,
                Is.EqualTo(requestedRole));
            Assert.That(
                verificationContext.Seats.Any(seat => seat.Id == memberSetup.SeatId
                    && seat.AssignedUserId == member.UserId),
                Is.True);
            Assert.That(
                verificationContext.DeviceActivations.Any(device =>
                    device.Id == activation.ActivationId
                    && device.SeatId == memberSetup.SeatId
                    && device.RevokedAt == null),
                Is.True);
            Assert.That(verificationContext.AuditRecords.Count(), Is.EqualTo(baselineAuditCount + 1));
            Assert.That(audit.Action, Is.EqualTo(AuditAction.OrganizationMemberRoleChanged));
            Assert.That(audit.TargetType, Is.EqualTo(AuditTargetType.OrganizationMembership));
            Assert.That(audit.TargetId, Is.EqualTo(memberSetup.MembershipId));
            Assert.That(audit.ActorUserId, Is.EqualTo(owner.UserId));
            Assert.That(audit.OccurredAt, Is.EqualTo(observedAt));
            Assert.That(audit.Id.Version, Is.EqualTo(7));
        });
    }

    [Test]
    public async Task OwnerTransfersOwnershipAndBecomesAdminWithoutMovingSeatsOrDevices()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "transfer-owner", "owner@example.com");
        var target = await SignUpAsync(database, "transfer-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Transfer Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        var ownerActivation = await ActivateAsync(
            database,
            owner.UserId,
            organization.SeatId,
            1,
            SignupTime.AddDays(2));
        var targetActivation = await ActivateAsync(
            database,
            target.UserId,
            targetSetup.SeatId,
            2,
            SignupTime.AddDays(2));
        var observedAt = SignupTime.AddDays(3);
        await using var context = database.CreateContext();
        var ownerMembership = await context.OrganizationMemberships.SingleAsync(
            membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == owner.UserId);

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, observedAt);
        var result = await roleTest.Service.TransferOwnershipAsync(
            owner.UserId,
            organization.OrganizationId,
            targetSetup.MembershipId);

        var success = RequireOwnershipTransfer(result);

        await using var verificationContext = database.CreateContext();
        var roles = await verificationContext.OrganizationMemberships
            .Where(membership => membership.OrganizationId == organization.OrganizationId)
            .ToDictionaryAsync(membership => membership.Id, membership => membership.Role);
        var audits = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == success.CorrelationId)
            .OrderBy(record => record.Action)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(success.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(success.PreviousOwnerMembershipId, Is.EqualTo(ownerMembership.Id));
            Assert.That(success.PreviousOwnerUserId, Is.EqualTo(owner.UserId));
            Assert.That(success.OwnerMembershipId, Is.EqualTo(targetSetup.MembershipId));
            Assert.That(success.OwnerUserId, Is.EqualTo(target.UserId));
            Assert.That(success.TransferredAt, Is.EqualTo(observedAt));
            Assert.That(success.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(roles[ownerMembership.Id], Is.EqualTo(OrganizationRole.Admin));
            Assert.That(roles[targetSetup.MembershipId], Is.EqualTo(OrganizationRole.Owner));
            Assert.That(roles.Values.Count(role => role == OrganizationRole.Owner), Is.EqualTo(1));
            Assert.That(
                verificationContext.Seats.Any(seat => seat.Id == organization.SeatId
                    && seat.AssignedUserId == owner.UserId),
                Is.True);
            Assert.That(
                verificationContext.Seats.Any(seat => seat.Id == targetSetup.SeatId
                    && seat.AssignedUserId == target.UserId),
                Is.True);
            Assert.That(
                verificationContext.DeviceActivations.Any(device =>
                    device.Id == ownerActivation.ActivationId
                    && device.SeatId == organization.SeatId
                    && device.RevokedAt == null),
                Is.True);
            Assert.That(
                verificationContext.DeviceActivations.Any(device =>
                    device.Id == targetActivation.ActivationId
                    && device.SeatId == targetSetup.SeatId
                    && device.RevokedAt == null),
                Is.True);
            Assert.That(
                audits.Select(audit => (audit.Action, audit.TargetId)),
                Is.EquivalentTo(new[]
                {
                    (AuditAction.OrganizationMemberRoleChanged, ownerMembership.Id),
                    (AuditAction.OrganizationOwnerAssigned, targetSetup.MembershipId),
                }));
            Assert.That(audits.All(audit => audit.ActorUserId == owner.UserId), Is.True);
            Assert.That(audits.All(audit => audit.OccurredAt == observedAt), Is.True);
            Assert.That(audits.All(audit => audit.Id.Version == 7), Is.True);
        });
    }

    [Test]
    public async Task RejectedRoleOperationsDoNotChangeMembershipsOrWriteAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "reject-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "reject-admin", "admin@example.com");
        var member = await SignUpAsync(database, "reject-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Rejected Role Organization",
            SignupTime.AddDays(1));
        var adminSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await using var context = database.CreateContext();
        var ownerMembership = await context.OrganizationMemberships.SingleAsync(
            membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == owner.UserId);
        var baselineAuditCount = await context.AuditRecords.CountAsync();
        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var service = roleTest.Service;

        var unauthorized = await service.ChangeRoleAsync(
            admin.UserId,
            organization.OrganizationId,
            memberSetup.MembershipId,
            OrganizationRole.Admin);
        var unchanged = await service.ChangeRoleAsync(
            owner.UserId,
            organization.OrganizationId,
            adminSetup.MembershipId,
            OrganizationRole.Admin);
        var ownerChange = await service.ChangeRoleAsync(
            owner.UserId,
            organization.OrganizationId,
            ownerMembership.Id,
            OrganizationRole.Member);
        var invalidTransfer = await service.TransferOwnershipAsync(
            owner.UserId,
            organization.OrganizationId,
            ownerMembership.Id);

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(unauthorized.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.InsufficientPermission));
            Assert.That(unchanged.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.RoleUnchanged));
            Assert.That(ownerChange.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.OwnershipTransferRequired));
            Assert.That(invalidTransfer.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.InvalidOwnershipTarget));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == ownerMembership.Id).Role,
                Is.EqualTo(OrganizationRole.Owner));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == adminSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Admin));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == memberSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Member));
            Assert.That(verificationContext.AuditRecords.Count(), Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task ConcurrentOwnershipTransfersHaveExactlyOneWinner()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "race-owner", "owner@example.com");
        var firstTarget = await SignUpAsync(database, "race-first", "first@example.com");
        var secondTarget = await SignUpAsync(database, "race-second", "second@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Transfer Race Organization",
            SignupTime.AddDays(1));
        var firstSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            firstTarget.UserId,
            OrganizationRole.Admin);
        var secondSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            secondTarget.UserId,
            OrganizationRole.Member);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var firstContext = database.CreateContext();
        var baselineAuditCount = await firstContext.AuditRecords.CountAsync();

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [
            new OrganizationRoleUpdateBarrierInterceptor(barrier)]);
        await using var roleTest2 = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [
            new OrganizationRoleUpdateBarrierInterceptor(barrier)]);
        var results = await Task.WhenAll(
            roleTest.Service.TransferOwnershipAsync(
                owner.UserId,
                organization.OrganizationId,
                firstSetup.MembershipId),
            roleTest2.Service.TransferOwnershipAsync(
                owner.UserId,
                organization.OrganizationId,
                secondSetup.MembershipId));

        await using var verificationContext = database.CreateContext();
        var success = RequireOwnershipTransfer(
            results.Single(result =>
                result.Status == OrganizationRoleManagementStatus.OwnershipTransferred));
        var transferAudits = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == success.CorrelationId)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationRoleManagementStatus.OwnershipTransferred),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationRoleManagementStatus.InsufficientPermission),
                Is.EqualTo(1));
            Assert.That(
                verificationContext.OrganizationMemberships.Count(membership =>
                    membership.OrganizationId == organization.OrganizationId
                    && membership.Role == OrganizationRole.Owner),
                Is.EqualTo(1));
            Assert.That(
                transferAudits.Select(audit => (audit.Action, audit.TargetId)),
                Is.EquivalentTo(new[]
                {
                    (AuditAction.OrganizationMemberRoleChanged,
                        success.PreviousOwnerMembershipId),
                    (AuditAction.OrganizationOwnerAssigned, success.OwnerMembershipId),
                }));
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.CorrelationId == success.CorrelationId),
                Is.EqualTo(2));
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount + 2));
        });
    }

    [Test]
    public async Task ConcurrentTransferAndFormerOwnerRemovalUseConsistentRoleSnapshots()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "transfer-remove-owner",
            "owner@example.com");
        var target = await SignUpAsync(
            database,
            "transfer-remove-target",
            "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Transfer Remove Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Admin);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var observedAt = SignupTime.AddDays(2);

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, observedAt, interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "FROM organization_memberships")]);
        var transferTask = roleTest.Service
            .TransferOwnershipAsync(
                owner.UserId,
                organization.OrganizationId,
                targetSetup.MembershipId);
        await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database, observedAt, interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "FROM organization_memberships")]);
        var removalTask = removalTest.Service
            .RemoveAsync(
                target.UserId,
                organization.OrganizationId,
                organization.OwnerMembershipId);
        await Task.WhenAll(transferTask, removalTask);

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                transferTask.Result.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.OwnershipTransferred));
            Assert.That(
                removalTask.Result.Status,
                Is.EqualTo(OrganizationMemberRemovalStatus.OwnershipTransferRequired));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == organization.OwnerMembershipId).Role,
                Is.EqualTo(OrganizationRole.Admin));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == targetSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Owner));
            Assert.That(
                verificationContext.OrganizationMemberships.Count(membership =>
                    membership.OrganizationId == organization.OrganizationId
                    && membership.Role == OrganizationRole.Owner),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task FailureAfterBothTransferWritesRollsBackRolesAndAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "rollback-owner", "owner@example.com");
        var target = await SignUpAsync(database, "rollback-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Transfer Rollback Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Admin);
        await using var context = database.CreateContext();
        var ownerMembershipId = await context.OrganizationMemberships
            .Where(membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == owner.UserId)
            .Select(membership => membership.Id)
            .SingleAsync();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [new ThrowAfterSaveInterceptor()]);
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await roleTest.Service
                .TransferOwnershipAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    targetSetup.MembershipId));

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == ownerMembershipId).Role,
                Is.EqualTo(OrganizationRole.Owner));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == targetSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Admin));
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task FailureAfterRoleChangeWriteRollsBackRoleAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "role-rollback-owner", "owner@example.com");
        var target = await SignUpAsync(database, "role-rollback-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role Rollback Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [new ThrowAfterSaveInterceptor()]);
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await roleTest.Service.ChangeRoleAsync(
                owner.UserId,
                organization.OrganizationId,
                targetSetup.MembershipId,
                OrganizationRole.Admin));

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == targetSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Member));
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task RepeatedTransferTargetConflictsRollBackDemotionAndAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "transfer-conflict-owner", "owner@example.com");
        var target = await SignUpAsync(database, "transfer-conflict-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Transfer Conflict Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Admin);
        var interceptor = new OrganizationRoleUpdateConflictInterceptor(
            suppressEveryUpdate: false);
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [interceptor]);
        var result = await roleTest.Service
            .TransferOwnershipAsync(
                owner.UserId,
                organization.OrganizationId,
                targetSetup.MembershipId);

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.ConcurrentModification));
            Assert.That(interceptor.UpdateCount, Is.EqualTo(6));
            Assert.That(interceptor.SuppressedUpdateCount, Is.EqualTo(3));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == organization.OwnerMembershipId).Role,
                Is.EqualTo(OrganizationRole.Owner));
            Assert.That(
                verificationContext.OrganizationMemberships.Single(
                    membership => membership.Id == targetSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Admin));
            Assert.That(
                verificationContext.OrganizationMemberships.Count(membership =>
                    membership.OrganizationId == organization.OrganizationId
                    && membership.Role == OrganizationRole.Owner),
                Is.EqualTo(1));
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task RepeatedOptimisticConflictsReturnAStableResultWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "conflict-owner", "owner@example.com");
        var target = await SignUpAsync(database, "conflict-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role Conflict Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        var interceptor = new OrganizationRoleUpdateConflictInterceptor();
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [interceptor]);
        var result = await roleTest.Service.ChangeRoleAsync(
            owner.UserId,
            organization.OrganizationId,
            targetSetup.MembershipId,
            OrganizationRole.Admin);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.ConcurrentModification));
            Assert.That(interceptor.SuppressedUpdateCount, Is.EqualTo(3));
            Assert.That(
                context.OrganizationMemberships.Single(
                    membership => membership.Id == targetSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Member));
            Assert.That(context.AuditRecords.Count(), Is.EqualTo(baselineAuditCount));
        });
    }

    private static OrganizationRoleManagementResult.RoleChanged RequireRoleChange(
        OrganizationRoleManagementResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationRoleManagementResult.RoleChanged>());
        return (OrganizationRoleManagementResult.RoleChanged)result;
    }

    private static OrganizationRoleManagementResult.OwnershipTransferred
        RequireOwnershipTransfer(OrganizationRoleManagementResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationRoleManagementResult.OwnershipTransferred>());
        return (OrganizationRoleManagementResult.OwnershipTransferred)result;
    }
}
