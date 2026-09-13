using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationRoleManagementPolicyTests
{
    [TestCase(OrganizationRole.Admin, OrganizationRole.Member)]
    [TestCase(OrganizationRole.Member, OrganizationRole.Admin)]
    public void OwnerCanPromoteOrDemoteANonOwner(
        OrganizationRole currentRole,
        OrganizationRole requestedRole)
    {
        var decision = OrganizationRoleManagementPolicy.DecideRoleChange(
            OrganizationRole.Owner,
            currentRole,
            requestedRole);

        Assert.That(decision, Is.EqualTo(OrganizationRoleManagementDecision.Allowed));
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    [TestCase((OrganizationRole)999)]
    public void NonOwnerCannotChangeRoles(OrganizationRole actorRole)
    {
        var decision = OrganizationRoleManagementPolicy.DecideRoleChange(
            actorRole,
            OrganizationRole.Member,
            OrganizationRole.Admin);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationRoleManagementDecision.InsufficientPermission));
    }

    [Test]
    public void OwnerRoleCannotBeChangedThroughTheRoleEndpoint()
    {
        var decision = OrganizationRoleManagementPolicy.DecideRoleChange(
            OrganizationRole.Owner,
            OrganizationRole.Owner,
            OrganizationRole.Admin);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationRoleManagementDecision.OwnershipTransferRequired));
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public void NonOwnerCannotChangeTheOwnerRole(OrganizationRole actorRole)
    {
        var decision = OrganizationRoleManagementPolicy.DecideRoleChange(
            actorRole,
            OrganizationRole.Owner,
            OrganizationRole.Admin);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationRoleManagementDecision.InsufficientPermission));
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public void ReapplyingTheCurrentRoleIsNotAChange(OrganizationRole currentRole)
    {
        var decision = OrganizationRoleManagementPolicy.DecideRoleChange(
            OrganizationRole.Owner,
            currentRole,
            currentRole);

        Assert.That(decision, Is.EqualTo(OrganizationRoleManagementDecision.RoleUnchanged));
    }

    [TestCase(OrganizationRole.Owner)]
    [TestCase((OrganizationRole)999)]
    public void InvalidRequestedRolesAreRejected(OrganizationRole requestedRole)
    {
        var decision = OrganizationRoleManagementPolicy.DecideRoleChange(
            OrganizationRole.Owner,
            OrganizationRole.Member,
            requestedRole);

        Assert.That(decision, Is.EqualTo(OrganizationRoleManagementDecision.InvalidRole));
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public void OwnerCanTransferOwnershipToANonOwner(OrganizationRole targetRole)
    {
        var decision = OrganizationRoleManagementPolicy.DecideOwnershipTransfer(
            Guid.CreateVersion7(),
            OrganizationRole.Owner,
            Guid.CreateVersion7(),
            targetRole);

        Assert.That(decision, Is.EqualTo(OrganizationRoleManagementDecision.Allowed));
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    [TestCase((OrganizationRole)999)]
    public void NonOwnerCannotTransferOwnership(OrganizationRole actorRole)
    {
        var decision = OrganizationRoleManagementPolicy.DecideOwnershipTransfer(
            Guid.CreateVersion7(),
            actorRole,
            Guid.CreateVersion7(),
            OrganizationRole.Member);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationRoleManagementDecision.InsufficientPermission));
    }

    [Test]
    public void CurrentOwnerIsNotAValidTransferTarget()
    {
        var ownerUserId = Guid.CreateVersion7();

        var decision = OrganizationRoleManagementPolicy.DecideOwnershipTransfer(
            ownerUserId,
            OrganizationRole.Owner,
            ownerUserId,
            OrganizationRole.Owner);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationRoleManagementDecision.InvalidOwnershipTarget));
    }
}
