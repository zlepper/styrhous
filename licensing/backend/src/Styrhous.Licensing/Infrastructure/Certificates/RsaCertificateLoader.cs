using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;

namespace Styrhous.Licensing.Infrastructure.Certificates;

internal static class RsaCertificateLoader
{
    public static X509Certificate2 LoadPkcs12(
        string? encodedCertificate,
        string? certificatePassword,
        string configurationKey)
    {
        if (string.IsNullOrWhiteSpace(encodedCertificate))
        {
            throw new InvalidOperationException(
                $"{configurationKey} is required.");
        }

        try
        {
            var certificate = X509CertificateLoader.LoadPkcs12(
                Convert.FromBase64String(encodedCertificate),
                certificatePassword,
                X509KeyStorageFlags.EphemeralKeySet,
                Pkcs12LoaderLimits.Defaults);
            if (!certificate.HasPrivateKey)
            {
                certificate.Dispose();
                throw new InvalidOperationException(
                    $"{configurationKey} must contain a private key.");
            }

            using var publicKey = certificate.GetRSAPublicKey();
            using var privateKey = certificate.GetRSAPrivateKey();
            if (publicKey is null || privateKey is null)
            {
                certificate.Dispose();
                throw new InvalidOperationException(
                    $"{configurationKey} must contain an RSA public and private key.");
            }

            return certificate;
        }
        catch (Exception exception) when (exception is FormatException
            or CryptographicException)
        {
            throw new InvalidOperationException(
                $"{configurationKey} must be a valid base64-encoded PKCS#12 "
                    + "certificate with the configured password.",
                exception);
        }
    }
}
