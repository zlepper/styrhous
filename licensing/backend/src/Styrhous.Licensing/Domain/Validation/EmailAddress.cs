using System.Net.Mail;

namespace Styrhous.Licensing.Domain.Validation;

internal static class EmailAddress
{
    public const int MaximumLength = 320;

    public static bool IsValid(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return false;
        }

        var trimmed = value.Trim();
        return trimmed.Length <= MaximumLength
            && trimmed.ToUpperInvariant().Length <= MaximumLength
            && MailAddress.TryCreate(trimmed, out var parsed)
            && string.Equals(parsed.Address, trimmed, StringComparison.OrdinalIgnoreCase);
    }

    public static (string Value, string NormalizedValue) Normalize(
        string value,
        string parameterName)
    {
        var trimmed = RequiredText.Normalize(
            value,
            parameterName,
            MaximumLength,
            "email address");
        var normalized = trimmed.ToUpperInvariant();
        if (normalized.Length > MaximumLength)
        {
            throw new ArgumentException(
                $"The normalized email address must be no longer than {MaximumLength} characters.",
                parameterName);
        }

        return (trimmed, normalized);
    }
}
