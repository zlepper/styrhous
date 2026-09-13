using System.Security.Cryptography;
using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Infrastructure.Organizations;

public sealed class CryptographicOrganizationInvitationSecretGenerator

{
    private const int SecretByteLength = 32;

    public OrganizationInvitationSecret Generate()
    {
        var secretBytes = RandomNumberGenerator.GetBytes(SecretByteLength);
        var secret = Convert.ToBase64String(secretBytes)
            .TrimEnd('=')
            .Replace('+', '-')
            .Replace('/', '_');
        return new OrganizationInvitationSecret(secret);
    }
}
