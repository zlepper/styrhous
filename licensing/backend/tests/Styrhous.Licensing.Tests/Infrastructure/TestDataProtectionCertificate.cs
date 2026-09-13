using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;

namespace Styrhous.Licensing.Tests.Infrastructure;

internal static class TestDataProtectionCertificate
{
    public const string Password = "styrhous-test-certificate";

    private static readonly Lazy<string> EncodedCertificateSource = new(CreateRsaCertificate);
    private static readonly Lazy<string> EncodedDesktopCertificateSource =
        new(() => CreateDesktopProtocolRsaCertificate());

    public static string EncodedCertificate => EncodedCertificateSource.Value;

    public static string EncodedDesktopCertificate =>
        EncodedDesktopCertificateSource.Value;

    public static string CreateEncryptionOnlyRsaCertificate()
    {
        return CreateRsaCertificate();
    }

    public static string CreateDesktopProtocolRsaCertificate(
        DateTimeOffset? expiresAt = null,
        DateTimeOffset? validFrom = null)
    {
        using var rsa = RSA.Create(2048);
        var request = new CertificateRequest(
            "CN=Styrhous Desktop Protocol Tests",
            rsa,
            HashAlgorithmName.SHA256,
            RSASignaturePadding.Pkcs1);
        request.CertificateExtensions.Add(
            new X509KeyUsageExtension(
                X509KeyUsageFlags.KeyEncipherment | X509KeyUsageFlags.DigitalSignature,
                critical: true));
        using var certificate = request.CreateSelfSigned(
            validFrom ?? DateTimeOffset.UtcNow.AddYears(-1),
            expiresAt ?? DateTimeOffset.UtcNow.AddYears(2));
        return Convert.ToBase64String(
            certificate.Export(X509ContentType.Pkcs12, Password));
    }

    public static string CreateRsaCertificate()
    {
        using var rsa = RSA.Create(2048);
        var request = new CertificateRequest(
            "CN=Styrhous Licensing Tests",
            rsa,
            HashAlgorithmName.SHA256,
            RSASignaturePadding.Pkcs1);
        request.CertificateExtensions.Add(
            new X509KeyUsageExtension(
                X509KeyUsageFlags.KeyEncipherment,
                critical: true));
        using var certificate = request.CreateSelfSigned(
            DateTimeOffset.UtcNow.AddYears(-1),
            DateTimeOffset.UtcNow.AddYears(2));
        return Convert.ToBase64String(
            certificate.Export(X509ContentType.Pkcs12, Password));
    }

    public static string CreateEcdsaCertificate()
    {
        using var ecdsa = ECDsa.Create(ECCurve.NamedCurves.nistP256);
        var request = new CertificateRequest(
            "CN=Styrhous Licensing ECDSA Test",
            ecdsa,
            HashAlgorithmName.SHA256);
        using var certificate = request.CreateSelfSigned(
            DateTimeOffset.UtcNow.AddYears(-1),
            DateTimeOffset.UtcNow.AddYears(2));
        return Convert.ToBase64String(
            certificate.Export(X509ContentType.Pkcs12, Password));
    }
}
