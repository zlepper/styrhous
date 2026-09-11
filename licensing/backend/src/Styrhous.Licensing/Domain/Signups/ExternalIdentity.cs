using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Signups;

public sealed class ExternalIdentity
{
    private ExternalIdentity()
    {
    }

    private ExternalIdentity(
        Guid id,
        Guid userId,
        string provider,
        string subject,
        DateTimeOffset createdAt)
    {
        Id = id;
        UserId = userId;
        Provider = provider;
        Subject = subject;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public Guid UserId { get; private set; }

    public string Provider { get; private set; } = string.Empty;

    public string Subject { get; private set; } = string.Empty;

    public DateTimeOffset CreatedAt { get; private set; }

    internal static ExternalIdentity Create(
        Guid userId,
        VerifiedExternalIdentity identity,
        DateTimeOffset createdAt)
    {

        ArgumentNullException.ThrowIfNull(identity);
        return new ExternalIdentity(
            Uuid7.Create(),
            userId,
            identity.Provider,
            identity.Subject,
            createdAt.ToUniversalTime());
    }
}
