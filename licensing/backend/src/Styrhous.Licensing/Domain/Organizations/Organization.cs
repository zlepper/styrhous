using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Organizations;

public sealed class Organization
{
    public const int MaximumNameLength = 120;

    private Organization()
    {
    }

    private Organization(
        Guid id,
        Guid billingAccountId,
        Guid createdByUserId,
        string name,
        DateTimeOffset createdAt)
    {
        Id = id;
        BillingAccountId = billingAccountId;
        CreatedByUserId = createdByUserId;
        Name = name;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public Guid BillingAccountId { get; private set; }

    public Guid CreatedByUserId { get; private set; }

    public string Name { get; private set; } = string.Empty;

    public DateTimeOffset CreatedAt { get; private set; }

    public long ConcurrencyVersion { get; private set; }

    internal static Organization Create(
        Guid billingAccountId,
        Guid createdByUserId,
        string name,
        DateTimeOffset createdAt)
    {

        var normalizedName = RequiredText.Normalize(
            name,
            nameof(name),
            MaximumNameLength,
            "organization name");

        return new Organization(
            Uuid7.Create(),
            billingAccountId,
            createdByUserId,
            normalizedName,
            createdAt.ToUniversalTime());
    }
}
