using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Organizations;

public sealed class OrganizationRegistration
{
    private OrganizationRegistration(
        BillingAccount billingAccount,
        Organization organization,
        OrganizationMembership ownerMembership,
        Seat seat)
    {
        BillingAccount = billingAccount;
        Organization = organization;
        OwnerMembership = ownerMembership;
        Seat = seat;
    }

    public BillingAccount BillingAccount { get; }

    public Organization Organization { get; }

    public OrganizationMembership OwnerMembership { get; }

    public Seat Seat { get; }

    public DateTimeOffset ObservedAt { get; private init; }

    public static OrganizationRegistration Start(
        Guid ownerUserId,
        string name,
        DateTimeOffset observedAt)
    {

        var utcObservedAt = observedAt.ToUniversalTime();
        var billingAccount = BillingAccount.CreateOrganization(utcObservedAt);
        var organization = Organization.Create(
            billingAccount.Id,
            ownerUserId,
            name,
            utcObservedAt);
        var ownerMembership = OrganizationMembership.CreateOwner(
            organization.Id,
            ownerUserId,
            utcObservedAt);
        var seat = Seat.Assign(billingAccount.Id, ownerUserId, utcObservedAt);
        return new OrganizationRegistration(
            billingAccount,
            organization,
            ownerMembership,
            seat)
        {
            ObservedAt = utcObservedAt,
        };
    }
}
