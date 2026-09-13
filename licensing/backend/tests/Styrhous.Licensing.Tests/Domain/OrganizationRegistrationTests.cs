using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
public sealed class OrganizationRegistrationTests
{
    [Test]
    public void StartingAnOrganizationCreatesItsOwnerMembershipAndSeat()
    {
        var ownerUserId = Guid.CreateVersion7();
        var observedAt = new DateTimeOffset(2026, 8, 30, 12, 34, 56, TimeSpan.FromHours(2));

        var registration = OrganizationRegistration.Start(
            ownerUserId,
            "  Example Organization  ",
            observedAt);

        Assert.Multiple(() =>
        {
            Assert.That(registration.Organization.Name, Is.EqualTo("Example Organization"));
            Assert.That(registration.Organization.CreatedByUserId, Is.EqualTo(ownerUserId));
            Assert.That(
                registration.Organization.BillingAccountId,
                Is.EqualTo(registration.BillingAccount.Id));
            Assert.That(registration.BillingAccount.Kind, Is.EqualTo(BillingAccountKind.Organization));
            Assert.That(registration.BillingAccount.PersonalOwnerUserId, Is.Null);
            Assert.That(registration.OwnerMembership.Role, Is.EqualTo(OrganizationRole.Owner));
            Assert.That(
                registration.OwnerMembership.OrganizationId,
                Is.EqualTo(registration.Organization.Id));
            Assert.That(registration.OwnerMembership.UserId, Is.EqualTo(ownerUserId));
            Assert.That(
                registration.Seat.BillingAccountId,
                Is.EqualTo(registration.BillingAccount.Id));
            Assert.That(registration.Seat.AssignedUserId, Is.EqualTo(ownerUserId));
            Assert.That(registration.Organization.CreatedAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(registration.ObservedAt, Is.EqualTo(observedAt.ToUniversalTime()));
        });

        var ownedIds = new[]
        {
            registration.BillingAccount.Id,
            registration.Organization.Id,
            registration.OwnerMembership.Id,
            registration.Seat.Id,
        };
        Assert.Multiple(() =>
        {
            Assert.That(ownedIds, Is.Unique);
            Assert.That(ownedIds, Has.All.Property(nameof(Guid.Version)).EqualTo(7));
        });
    }

    [TestCase("36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase(TestIdentifiers.Version7WithNonRfcVariantText)]
    public void OrganizationPreservesExistingOwnerIdentifier(string ownerUserId)
    {
        var identifier = Guid.Parse(ownerUserId);
        var registration = OrganizationRegistration.Start(identifier, "Example Organization", DateTimeOffset.UtcNow);
        Assert.Multiple(() =>
        {
            Assert.That(registration.Organization.CreatedByUserId, Is.EqualTo(identifier));
            Assert.That(registration.OwnerMembership.UserId, Is.EqualTo(identifier));
        });
    }

    [TestCase("")]
    [TestCase("   ")]
    public void OrganizationNameIsRequired(string name)
    {
        Assert.That(
            () => OrganizationRegistration.Start(Guid.CreateVersion7(), name, DateTimeOffset.UtcNow),
            Throws.TypeOf<ArgumentException>());
    }

    [Test]
    public void OrganizationNameCannotExceedItsPersistenceLimit()
    {
        Assert.That(
            () => OrganizationRegistration.Start(
                Guid.CreateVersion7(),
                new string('o', Organization.MaximumNameLength + 1),
                DateTimeOffset.UtcNow),
            Throws.TypeOf<ArgumentException>());
    }
}
