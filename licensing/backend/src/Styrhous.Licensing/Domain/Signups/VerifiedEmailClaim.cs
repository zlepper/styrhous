using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Signups;

public sealed class VerifiedEmailClaim
{
    private VerifiedEmailClaim()
    {
    }

    private VerifiedEmailClaim(
        Guid id,
        Guid userId,
        string normalizedEmail,
        DateTimeOffset createdAt)
    {
        Id = id;
        UserId = userId;
        NormalizedEmail = normalizedEmail;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public Guid UserId { get; private set; }

    public string NormalizedEmail { get; private set; } = string.Empty;

    public DateTimeOffset CreatedAt { get; private set; }

    internal static VerifiedEmailClaim Create(
        Guid userId,
        VerifiedExternalIdentity identity,
        DateTimeOffset createdAt)
    {

        ArgumentNullException.ThrowIfNull(identity);
        return new VerifiedEmailClaim(
            Uuid7.Create(),
            userId,
            identity.NormalizedEmail,
            createdAt.ToUniversalTime());
    }
}
