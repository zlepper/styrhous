using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationMemberRemovalPolicyTests
{
    [TestCase(OrganizationRole.Owner, OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Owner, OrganizationRole.Member)]
    [TestCase(OrganizationRole.Admin, OrganizationRole.Member)]
    public void AuthorizedRolePairsCanRemoveAnotherMember(
        OrganizationRole actorRole,
        OrganizationRole targetRole)
    {
        var decision = OrganizationMemberRemovalPolicy.Decide(
            Guid.CreateVersion7(),
            actorRole,
            Guid.CreateVersion7(),
            targetRole);

        Assert.That(decision, Is.EqualTo(OrganizationMemberRemovalDecision.Allowed));
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public void NonOwnersCanLeaveTheOrganization(OrganizationRole role)
    {
        var userId = Guid.CreateVersion7();

        var decision = OrganizationMemberRemovalPolicy.Decide(
            userId,
            role,
            userId,
            role);

        Assert.That(decision, Is.EqualTo(OrganizationMemberRemovalDecision.Allowed));
    }

    [TestCase(OrganizationRole.Admin, OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member, OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member, OrganizationRole.Member)]
    [TestCase((OrganizationRole)999, OrganizationRole.Member)]
    public void UnauthorizedRolePairsCannotRemoveAnotherMember(
        OrganizationRole actorRole,
        OrganizationRole targetRole)
    {
        var decision = OrganizationMemberRemovalPolicy.Decide(
            Guid.CreateVersion7(),
            actorRole,
            Guid.CreateVersion7(),
            targetRole);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationMemberRemovalDecision.InsufficientPermission));
    }

    [TestCase(OrganizationRole.Owner)]
    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public void OwnerMustNeverBeRemovedBeforeOwnershipIsTransferred(
        OrganizationRole actorRole)
    {
        var decision = OrganizationMemberRemovalPolicy.Decide(
            Guid.CreateVersion7(),
            actorRole,
            Guid.CreateVersion7(),
            OrganizationRole.Owner);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationMemberRemovalDecision.OwnershipTransferRequired));
    }

    [Test]
    public void OwnerMustTransferOwnershipBeforeLeaving()
    {
        var ownerUserId = Guid.CreateVersion7();

        var decision = OrganizationMemberRemovalPolicy.Decide(
            ownerUserId,
            OrganizationRole.Owner,
            ownerUserId,
            OrganizationRole.Owner);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationMemberRemovalDecision.OwnershipTransferRequired));
    }
}
