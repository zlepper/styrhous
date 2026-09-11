using System.Security.Cryptography;
using System.Text;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationInvitationSecret
{
    public const int MaximumLength = 256;

    private const string RedactedValue = "[REDACTED]";

    private readonly string _value;

    public OrganizationInvitationSecret(string value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(value);
        if (value.Length > MaximumLength)
        {
            throw new ArgumentException(
                $"An invitation secret cannot exceed {MaximumLength} characters.",
                nameof(value));
        }

        _value = value;
        Hash = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value)))
            .ToLowerInvariant();
    }

    public string Hash { get; }

    public bool MatchesHash(string hash)
    {
        if (string.IsNullOrWhiteSpace(hash))
        {
            return false;
        }

        try
        {
            return CryptographicOperations.FixedTimeEquals(
                Convert.FromHexString(Hash),
                Convert.FromHexString(hash));
        }
        catch (FormatException)
        {
            return false;
        }
    }

    public string Reveal()
    {
        return _value;
    }

    public override string ToString()
    {
        return RedactedValue;
    }
}
