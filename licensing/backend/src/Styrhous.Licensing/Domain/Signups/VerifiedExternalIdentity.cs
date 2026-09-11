using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Signups;

public sealed record VerifiedExternalIdentity
{
    public const int MaximumEmailLength = EmailAddress.MaximumLength;

    public const int MaximumProviderLength = 64;

    public const int MaximumSubjectLength = 512;

    private VerifiedExternalIdentity(
        string provider,
        string subject,
        string verifiedEmail,
        string normalizedEmail)
    {
        Provider = provider;
        Subject = subject;
        VerifiedEmail = verifiedEmail;
        NormalizedEmail = normalizedEmail;
    }

    public string Provider { get; }

    public string Subject { get; }

    public string VerifiedEmail { get; }

    public string NormalizedEmail { get; }

    public static VerifiedExternalIdentity Create(
        string provider,
        string subject,
        string verifiedEmail)
    {
        if (!EmailAddress.IsValid(verifiedEmail))
        {
            throw new ArgumentException(
                "The verified email address is invalid.",
                nameof(verifiedEmail));
        }

        var normalizedProvider = RequiredText.Normalize(
            provider,
            nameof(provider),
            MaximumProviderLength).ToLowerInvariant();
        var normalizedSubject = RequiredText.Normalize(
            subject,
            nameof(subject),
            MaximumSubjectLength);
        var (trimmedEmail, normalizedEmail) = EmailAddress.Normalize(
            verifiedEmail,
            nameof(verifiedEmail));

        return new VerifiedExternalIdentity(
            normalizedProvider,
            normalizedSubject,
            trimmedEmail,
            normalizedEmail);
    }
}
