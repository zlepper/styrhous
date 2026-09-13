using System.Net.Mail;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed class InvitationEmailSettings
{
    public const string SectionName = "InvitationEmail";

    public InvitationEmailSettings(string fromAddress, Uri acceptanceUrl)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(fromAddress);
        ArgumentNullException.ThrowIfNull(acceptanceUrl);
        var trimmedFromAddress = fromAddress.Trim();
        if (!MailAddress.TryCreate(trimmedFromAddress, out var parsedAddress)
            || !string.Equals(
                parsedAddress.Address,
                trimmedFromAddress,
                StringComparison.OrdinalIgnoreCase))
        {
            throw new ArgumentException(
                "A plain, valid invitation sender email address is required.",
                nameof(fromAddress));
        }

        if (!acceptanceUrl.IsAbsoluteUri
            || acceptanceUrl.Scheme != Uri.UriSchemeHttps
            || !string.IsNullOrEmpty(acceptanceUrl.Query)
            || !string.IsNullOrEmpty(acceptanceUrl.Fragment))
        {
            throw new ArgumentException(
                "The invitation acceptance URL must be an absolute HTTPS URL without a query or fragment.",
                nameof(acceptanceUrl));
        }

        FromAddress = parsedAddress.Address;
        AcceptanceUrl = acceptanceUrl;
    }

    public string FromAddress { get; }

    public Uri AcceptanceUrl { get; }
}

internal sealed class SmtpEmailSettings
{
    public SmtpEmailSettings(string host, int port, string username, string password)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(host);
        ArgumentException.ThrowIfNullOrWhiteSpace(username);
        ArgumentException.ThrowIfNullOrWhiteSpace(password);
        if (port is < 1 or > 65535)
        {
            throw new ArgumentOutOfRangeException(nameof(port));
        }

        Host = host.Trim();
        Port = port;
        Username = username.Trim();
        Password = password;
    }

    public string Host { get; }

    public int Port { get; }

    public string Username { get; }

    public string Password { get; }
}
