using System.Security.Cryptography.X509Certificates;
using Styrhous.Licensing.Infrastructure.Certificates;

namespace Styrhous.Licensing.Api.Desktop;

internal static class DesktopProtocolCertificateConfiguration
{
    public const string CertificateConfigurationKey =
        "DesktopProtocol:Certificate";

    public const string CertificatePasswordConfigurationKey =
        "DesktopProtocol:CertificatePassword";

    public const string PreviousCertificatesConfigurationKey =
        "DesktopProtocol:PreviousCertificates";

    public const string IssuerConfigurationKey = "DesktopProtocol:Issuer";

    public static Uri LoadIssuer(IConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        var configuredIssuer = configuration[IssuerConfigurationKey];
        if (!Uri.TryCreate(configuredIssuer, UriKind.Absolute, out var issuer)
            || issuer.Scheme != Uri.UriSchemeHttps
            || !string.IsNullOrEmpty(issuer.UserInfo)
            || issuer.AbsolutePath != "/"
            || !string.IsNullOrEmpty(issuer.Query)
            || !string.IsNullOrEmpty(issuer.Fragment))
        {
            throw new InvalidOperationException(
                $"{IssuerConfigurationKey} must be an absolute HTTPS origin with no path, "
                    + "query, fragment, or user information.");
        }

        return issuer;
    }

    public static DesktopProtocolCertificateRing LoadCertificateRing(
        IConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        var current = LoadCertificate(
            configuration[CertificateConfigurationKey],
            configuration[CertificatePasswordConfigurationKey],
            CertificateConfigurationKey);
        var previous = configuration
            .GetSection(PreviousCertificatesConfigurationKey)
            .GetChildren()
            .Select(section => LoadCertificate(
                section["Certificate"],
                section["CertificatePassword"],
                $"{section.Path}:Certificate"))
            .ToArray();
        var now = DateTime.UtcNow;
        if (current.NotBefore.ToUniversalTime() > now
            || current.NotAfter.ToUniversalTime() <= now)
        {
            Dispose(current, previous);
            throw new InvalidOperationException(
                $"{CertificateConfigurationKey} must be currently valid.");
        }

        if (previous.Any(certificate => certificate.NotAfter >= current.NotAfter))
        {
            Dispose(current, previous);
            throw new InvalidOperationException(
                $"{CertificateConfigurationKey} must expire after every retained previous "
                    + "desktop protocol certificate so OpenIddict selects it for issuance.");
        }

        return new DesktopProtocolCertificateRing(current, previous);
    }

    private static X509Certificate2 LoadCertificate(
        string? encodedCertificate,
        string? password,
        string configurationKey)
    {
        var certificate = RsaCertificateLoader.LoadPkcs12(
            encodedCertificate,
            password,
            configurationKey);
        var keyUsage = certificate.Extensions
            .OfType<X509KeyUsageExtension>()
            .SingleOrDefault();
        const X509KeyUsageFlags required =
            X509KeyUsageFlags.DigitalSignature | X509KeyUsageFlags.KeyEncipherment;
        if (keyUsage is not null && (keyUsage.KeyUsages & required) != required)
        {
            certificate.Dispose();
            throw new InvalidOperationException(
                $"{configurationKey} must allow digital signatures and key encipherment.");
        }

        return certificate;
    }

    private static void Dispose(
        X509Certificate2 current,
        IEnumerable<X509Certificate2> previous)
    {
        current.Dispose();
        foreach (var certificate in previous)
        {
            certificate.Dispose();
        }
    }
}

internal sealed record DesktopProtocolCertificateRing(
    X509Certificate2 Current,
    IReadOnlyList<X509Certificate2> Previous);
