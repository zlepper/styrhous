using System.Security.Cryptography.X509Certificates;
using Microsoft.AspNetCore.DataProtection;
using Microsoft.AspNetCore.DataProtection.EntityFrameworkCore;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Styrhous.Licensing.Infrastructure.Certificates;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Infrastructure.DataProtection;

public static class DataProtectionConfiguration
{
    public const string CertificateConfigurationKey =
        "DataProtection:Certificate";

    public const string CertificatePasswordConfigurationKey =
        "DataProtection:CertificatePassword";

    public const string PreviousCertificatesConfigurationKey =
        "DataProtection:PreviousCertificates";

    public const string ApplicationName = "Styrhous.Licensing";

    public static void Configure(
        IServiceCollection services,
        IConfiguration configuration)
    {
        Configure(services, LoadCertificateRing(configuration));
    }

    private static void Configure(
        IServiceCollection services,
        DataProtectionCertificateRing certificates)
    {
        ArgumentNullException.ThrowIfNull(services);
        ArgumentNullException.ThrowIfNull(certificates);
        var builder = services.AddDataProtection()
            .SetApplicationName(ApplicationName)
            .PersistKeysToDbContext<LicensingDbContext>()
            .ProtectKeysWithCertificate(certificates.Current);
        if (certificates.Previous.Count > 0)
        {
            builder.UnprotectKeysWithAnyCertificate([.. certificates.Previous]);
        }
    }

    private static DataProtectionCertificateRing LoadCertificateRing(
        IConfiguration configuration)
    {
        return new(
            LoadCurrentCertificate(configuration),
            LoadPreviousCertificates(configuration));
    }

    private static X509Certificate2 LoadCurrentCertificate(IConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        return RsaCertificateLoader.LoadPkcs12(
            configuration[CertificateConfigurationKey],
            configuration[CertificatePasswordConfigurationKey],
            CertificateConfigurationKey);
    }

    private static X509Certificate2[] LoadPreviousCertificates(
        IConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        return configuration
            .GetSection(PreviousCertificatesConfigurationKey)
            .GetChildren()
            .Select(section => RsaCertificateLoader.LoadPkcs12(
                section["Certificate"],
                section["CertificatePassword"],
                $"{section.Path}:Certificate"))
            .ToArray();
    }
}

internal sealed record DataProtectionCertificateRing(
    X509Certificate2 Current,
    IReadOnlyList<X509Certificate2> Previous);
