using System.Globalization;
using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.IdentityModel.JsonWebTokens;
using Microsoft.IdentityModel.Tokens;
using Styrhous.Licensing.Api.Desktop;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DesktopOfflineLeaseContractTests
{
    [Test]
    public async Task SharedGoldenLeaseHasThePublishedRs256Contract()
    {
        await using var fixture = File.OpenRead(Path.Combine(
            AppContext.BaseDirectory,
            "Fixtures",
            "Protocol",
            "offline-lease-v1.json"));
        using var document = await JsonDocument.ParseAsync(fixture);
        var root = document.RootElement;
        var key = root.GetProperty("keys").GetProperty("keys")[0];
        var parameters = new RSAParameters
        {
            Modulus = Base64UrlEncoder.DecodeBytes(key.GetProperty("n").GetString()),
            Exponent = Base64UrlEncoder.DecodeBytes(key.GetProperty("e").GetString()),
        };
        using var rsa = RSA.Create(parameters);
        var validation = await new JsonWebTokenHandler().ValidateTokenAsync(
            root.GetProperty("lease").GetString(),
            new TokenValidationParameters
            {
                ValidIssuer = root.GetProperty("issuer").GetString(),
                ValidAudience = root.GetProperty("audience").GetString(),
                IssuerSigningKey = new RsaSecurityKey(rsa)
                {
                    KeyId = key.GetProperty("kid").GetString(),
                },
                ValidateLifetime = false,
            });
        var issuedAt = long.Parse(
            validation.Claims["iat"].ToString()!,
            CultureInfo.InvariantCulture);
        var expiresAt = long.Parse(
            validation.Claims["exp"].ToString()!,
            CultureInfo.InvariantCulture);
        var notBefore = long.Parse(
            validation.Claims["nbf"].ToString()!,
            CultureInfo.InvariantCulture);
        var jwtId = Guid.Parse(validation.Claims["jti"].ToString()!);

        Assert.Multiple(() =>
        {
            Assert.That(validation.IsValid, Is.True, validation.Exception?.ToString());
            Assert.That(
                validation.Claims["installation_id"].ToString(),
                Is.EqualTo(root.GetProperty("installationId").GetString()));
            Assert.That(
                validation.Claims["state"].ToString(),
                Is.EqualTo(root.GetProperty("expected").GetProperty("state").GetString()));
            Assert.That(
                validation.Claims["reason_code"].ToString(),
                Is.EqualTo(root.GetProperty("expected").GetProperty("reasonCode").GetString()));
            Assert.That(notBefore, Is.EqualTo(issuedAt));
            Assert.That(
                expiresAt - issuedAt,
                Is.EqualTo((long)DesktopProtocolConstants.LeaseLifetime.TotalSeconds));
            Assert.That(jwtId.Version, Is.EqualTo(7));
        });
    }
}
