using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Accounts;

public enum BillingAccountKind
{
    Personal,
    Organization,
}

public sealed class BillingAccount
{
    private BillingAccount()
    {
    }

    private BillingAccount(
        Guid id,
        BillingAccountKind kind,
        Guid? personalOwnerUserId,
        DateTimeOffset createdAt)
    {
        Id = id;
        Kind = kind;
        PersonalOwnerUserId = personalOwnerUserId;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public BillingAccountKind Kind { get; private set; }

    public Guid? PersonalOwnerUserId { get; private set; }

    public DateTimeOffset CreatedAt { get; private set; }

    public long ConcurrencyVersion { get; private set; }

    public static BillingAccount CreatePersonal(Guid ownerUserId, DateTimeOffset createdAt)
    {

        return new BillingAccount(
            Uuid7.Create(),
            BillingAccountKind.Personal,
            ownerUserId,
            createdAt.ToUniversalTime());
    }

    public static BillingAccount CreateOrganization(DateTimeOffset createdAt)
    {
        return new(
            Uuid7.Create(),
            BillingAccountKind.Organization,
            personalOwnerUserId: null,
            createdAt.ToUniversalTime());
    }
}
