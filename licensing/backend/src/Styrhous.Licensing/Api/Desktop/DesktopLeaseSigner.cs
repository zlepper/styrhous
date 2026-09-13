using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.IdentityModel.Tokens;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Api.Desktop;

public sealed class DesktopLeaseSigner
{
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private readonly DesktopProtocolCertificateRing _certificates;
    private readonly Uri _issuer;

    internal DesktopLeaseSigner(
        DesktopProtocolCertificateRing certificates,
        Uri issuer)
    {
        _certificates = certificates;
        _issuer = issuer;
    }

    public DesktopLeaseIssueResult? TryIssue(
        Guid userId,
        DeviceEntitlementCheckResult check,
        DateTimeOffset observedAt)
    {
        if (check.Status is not DeviceEntitlementCheckStatus.Eligible
            || check.Entitlement is null
            || check.Device is null)
        {
            throw new ArgumentException("An eligible user entitlement is required.");
        }

        var now = observedAt.ToUniversalTime();
        var expiresAt = now.Add(DesktopProtocolConstants.LeaseLifetime);
        if (check.Entitlement.ValidUntil is { } entitlementEnd
            && entitlementEnd.ToUniversalTime() < expiresAt)
        {
            expiresAt = entitlementEnd.ToUniversalTime();
        }

        var issuedAtUnix = now.ToUnixTimeSeconds();
        var expiresAtUnix = expiresAt.ToUnixTimeSeconds();
        if (expiresAtUnix <= issuedAtUnix)
        {
            return null;
        }

        var refreshAfterUnix = Math.Min(
            now.Add(DesktopProtocolConstants.LeaseRefreshInterval).ToUnixTimeSeconds(),
            expiresAtUnix);
        var normalizedExpiresAt = DateTimeOffset.FromUnixTimeSeconds(expiresAtUnix);
        var normalizedRefreshAfter = DateTimeOffset.FromUnixTimeSeconds(refreshAfterUnix);

        var certificate = _certificates.Current;
        var keyId = KeyId(certificate);
        var header = EncodeJson(new LeaseHeader("RS256", "JWT", keyId));
        var payload = EncodeJson(new DesktopLeaseClaims(
            SchemaVersion: 1,
            Issuer: _issuer.AbsoluteUri,
            Audience: DesktopProtocolConstants.LeaseAudience,
            Subject: userId.ToString(),
            SeatId: check.Entitlement.SeatId.ToString(),
            BillingAccountId: check.Entitlement.BillingAccountId.ToString(),
            InstallationId: check.Device.InstallationId.ToString(),
            ActivationId: check.Device.ActivationId.ToString(),
            State: check.Entitlement.State.ToString().ToLowerInvariant(),
            ReasonCode: check.Entitlement.ReasonCode,
            ValidFrom: check.Entitlement.ValidFrom?.ToUnixTimeSeconds(),
            ValidUntil: check.Entitlement.ValidUntil?.ToUnixTimeSeconds(),
            IssuedAt: issuedAtUnix,
            NotBefore: issuedAtUnix,
            ExpiresAt: expiresAtUnix,
            RefreshAfter: refreshAfterUnix,
            JwtId: Uuid7.Create().ToString(),
            SigningKeyId: keyId));
        var signingInput = Encoding.ASCII.GetBytes($"{header}.{payload}");
        using var rsa = certificate.GetRSAPrivateKey()
            ?? throw new InvalidOperationException(
                "The desktop lease certificate has no RSA private key.");
        var signature = rsa.SignData(
            signingInput,
            HashAlgorithmName.SHA256,
            RSASignaturePadding.Pkcs1);
        return new DesktopLeaseIssueResult(
            $"{header}.{payload}.{Base64UrlEncoder.Encode(signature)}",
            normalizedExpiresAt,
            normalizedRefreshAfter,
            check.Entitlement.State.ToString().ToLowerInvariant(),
            check.Entitlement.ReasonCode);
    }

    public object GetJsonWebKeySet()
    {
        return new
        {
            keys = Certificates().Select(JsonWebKey).ToArray(),
        };
    }

    private IEnumerable<X509Certificate2> Certificates()
    {
        yield return _certificates.Current;
        foreach (var certificate in _certificates.Previous)
        {
            yield return certificate;
        }
    }

    private static object JsonWebKey(X509Certificate2 certificate)
    {
        using var rsa = certificate.GetRSAPublicKey()
            ?? throw new InvalidOperationException(
                "A desktop lease certificate has no RSA public key.");
        var parameters = rsa.ExportParameters(includePrivateParameters: false);
        return new
        {
            kty = "RSA",
            use = "sig",
            alg = "RS256",
            kid = KeyId(certificate),
            n = Base64UrlEncoder.Encode(parameters.Modulus),
            e = Base64UrlEncoder.Encode(parameters.Exponent),
        };
    }

    private static string KeyId(X509Certificate2 certificate)
    {
        return Base64UrlEncoder.Encode(SHA256.HashData(certificate.RawData));
    }

    private static string EncodeJson<T>(T value)
    {
        return Base64UrlEncoder.Encode(JsonSerializer.SerializeToUtf8Bytes(
            value,
            SerializerOptions));
    }

    private sealed record LeaseHeader(
        [property: JsonPropertyName("alg")] string Algorithm,
        [property: JsonPropertyName("typ")] string Type,
        [property: JsonPropertyName("kid")] string KeyId);

    private sealed record DesktopLeaseClaims(
        [property: JsonPropertyName("schema_version")] int SchemaVersion,
        [property: JsonPropertyName("iss")] string Issuer,
        [property: JsonPropertyName("aud")] string Audience,
        [property: JsonPropertyName("sub")] string Subject,
        [property: JsonPropertyName("seat_id")] string SeatId,
        [property: JsonPropertyName("billing_account_id")] string BillingAccountId,
        [property: JsonPropertyName("installation_id")] string InstallationId,
        [property: JsonPropertyName("activation_id")] string ActivationId,
        [property: JsonPropertyName("state")] string State,
        [property: JsonPropertyName("reason_code")] string ReasonCode,
        [property: JsonPropertyName("valid_from")] long? ValidFrom,
        [property: JsonPropertyName("valid_until")] long? ValidUntil,
        [property: JsonPropertyName("iat")] long IssuedAt,
        [property: JsonPropertyName("nbf")] long NotBefore,
        [property: JsonPropertyName("exp")] long ExpiresAt,
        [property: JsonPropertyName("refresh_after")] long RefreshAfter,
        [property: JsonPropertyName("jti")] string JwtId,
        [property: JsonPropertyName("signing_key_id")] string SigningKeyId);
}

public sealed record DesktopLeaseIssueResult(
    string Lease,
    DateTimeOffset ExpiresAt,
    DateTimeOffset RefreshAfter,
    string State,
    string ReasonCode);
