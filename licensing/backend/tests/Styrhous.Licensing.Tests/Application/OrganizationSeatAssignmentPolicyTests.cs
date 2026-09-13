using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationSeatAssignmentPolicyTests
{
    [TestCase(OrganizationRole.Owner)]
    [TestCase(OrganizationRole.Admin)]
    public void OwnerAndAdminCanChangeAnySeat(OrganizationRole actorRole)
    {
        var decision = OrganizationSeatAssignmentPolicy.Decide(
            actorRole,
            productAccessEnabled: false,
            requestedAssignment: true);

        Assert.That(decision, Is.EqualTo(OrganizationSeatAssignmentDecision.Allowed));
    }

    [TestCase(OrganizationRole.Member)]
    [TestCase((OrganizationRole)999)]
    public void OtherRolesCannotChangeSeats(OrganizationRole actorRole)
    {
        var decision = OrganizationSeatAssignmentPolicy.Decide(
            actorRole,
            productAccessEnabled: false,
            requestedAssignment: true);

        Assert.That(
            decision,
            Is.EqualTo(OrganizationSeatAssignmentDecision.InsufficientPermission));
    }

    [Test]
    public void ReapplyingTheCurrentAssignmentIsNotAChange()
    {
        var decision = OrganizationSeatAssignmentPolicy.Decide(
            OrganizationRole.Admin,
            productAccessEnabled: true,
            requestedAssignment: true);

        Assert.That(decision, Is.EqualTo(OrganizationSeatAssignmentDecision.Unchanged));
    }
}
