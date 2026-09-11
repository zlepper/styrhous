using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Signups;

public sealed class UserAccount
{
    private UserAccount()
    {
    }

    private UserAccount(
        Guid id,
        string verifiedEmail,
        string normalizedEmail,
        DateTimeOffset createdAt)
    {
        Id = id;
        VerifiedEmail = verifiedEmail;
        NormalizedEmail = normalizedEmail;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public string VerifiedEmail { get; private set; } = string.Empty;

    public string NormalizedEmail { get; private set; } = string.Empty;

    public DateTimeOffset CreatedAt { get; private set; }

    public long ConcurrencyVersion { get; private set; }

    internal static UserAccount Create(
        VerifiedExternalIdentity identity,
        DateTimeOffset createdAt)
    {
        return new(
            Uuid7.Create(),
            identity.VerifiedEmail,
            identity.NormalizedEmail,
            createdAt.ToUniversalTime());
    }

    internal void UpdateVerifiedEmail(VerifiedExternalIdentity identity)
    {
        ArgumentNullException.ThrowIfNull(identity);
        VerifiedEmail = identity.VerifiedEmail;
        NormalizedEmail = identity.NormalizedEmail;
    }
}
