using System.Text.Json;
using Pulumi;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure;

public sealed partial class LicensingInfrastructure
{
    private static Output<string> BuildDatabaseCredentials(
        string username,
        Output<string> databasePassword)
    {
        return databasePassword.Apply(password => JsonSerializer.Serialize(new
        {
            username,
            password,
        }));
    }

    private static RuntimeSecrets CreateRuntimeSecrets(
        string environment,
        Config config,
        string hostedOrigin,
        Output<string> databaseProxyEndpoint,
        Output<string> databasePassword,
        Output<string> databaseRuntimePassword)
    {
        var migrationConnectionString = BuildConnectionString(
            databaseProxyEndpoint,
            databasePassword,
            DatabaseMasterUsername);
        var runtimeConnectionString = BuildConnectionString(
            databaseProxyEndpoint,
            databaseRuntimePassword,
            DatabaseRuntimeUsername);
        return new RuntimeSecrets(
            CreateSecret(
                "licensing-api-configuration",
                environment,
                BuildApiApplicationConfiguration(config, hostedOrigin, runtimeConnectionString)),
            CreateSecret(
                "licensing-worker-configuration",
                environment,
                BuildWorkerApplicationConfiguration(config, hostedOrigin, runtimeConnectionString)),
            CreateSecret(
                "licensing-maintenance-configuration",
                environment,
                BuildConnectionConfiguration(runtimeConnectionString)),
            CreateSecret(
                "licensing-migration-configuration",
                environment,
                BuildConnectionConfiguration(migrationConnectionString)),
            CreateSecret(
                "licensing-provisioning-configuration",
                environment,
                BuildProvisioningConfiguration(
                    migrationConnectionString,
                    databaseRuntimePassword)));
    }

    private static Output<string> BuildApiApplicationConfiguration(
        Config config,
        string hostedOrigin,
        Output<string> connectionString)
    {
        var github = ReadAuthenticationProvider(config, "github");
        var google = ReadAuthenticationProvider(config, "google");
        var microsoft = ReadAuthenticationProvider(config, "microsoft");
        if (!github.Configured && !google.Configured && !microsoft.Configured)
        {
            throw new InvalidOperationException(
                "At least one complete external authentication provider is required.");
        }
        Input<string>[] values =
        [
            connectionString,
            config.RequireSecret("desktopCertificate"),
            config.RequireSecret("desktopCertificatePassword"),
            config.RequireSecret("dataProtectionCertificate"),
            config.RequireSecret("dataProtectionCertificatePassword"),
            config.RequireSecret("stripeSecretKey"),
            config.RequireSecret("stripeWebhookSecret"),
            github.Secret,
            google.Secret,
            microsoft.Secret,
            config.GetSecret("desktopPreviousCertificates")
                ?? Output.CreateSecret("[]"),
            config.GetSecret("dataProtectionPreviousCertificates")
                ?? Output.CreateSecret("[]"),
        ];
        return Output.All(values).Apply(items => JsonSerializer.Serialize(new
        {
            ConnectionStrings = new
            {
                Licensing = items[0],
            },
            DesktopProtocol = new
            {
                Certificate = items[1],
                CertificatePassword = items[2],
                Issuer = hostedOrigin,
                PreviousCertificates = ParsePreviousCertificates(
                    items[10],
                    "desktopPreviousCertificates"),
            },
            DataProtection = new
            {
                Certificate = items[3],
                CertificatePassword = items[4],
                PreviousCertificates = ParsePreviousCertificates(
                    items[11],
                    "dataProtectionPreviousCertificates"),
            },
            Stripe = new
            {
                SecretKey = items[5],
                WebhookSecret = items[6],
                MonthlyPriceId = config.Require("stripeMonthlyPriceId"),
                AnnualPriceId = config.Require("stripeAnnualPriceId"),
                CheckoutSuccessUrl = $"{hostedOrigin}/billing?checkout=success",
                CheckoutCancelUrl = $"{hostedOrigin}/billing?checkout=cancelled",
                CustomerPortalConfigurationId = config.Require("stripeCustomerPortalConfigurationId"),
                CustomerPortalReturnUrl = $"{hostedOrigin}/billing",
            },
            Authentication = new
            {
                GitHub = new { github.ClientId, ClientSecret = items[7] },
                Google = new { google.ClientId, ClientSecret = items[8] },
                Microsoft = new { microsoft.ClientId, ClientSecret = items[9] },
            },
        }));
    }

    private static Output<string> BuildWorkerApplicationConfiguration(
        Config config,
        string hostedOrigin,
        Output<string> connectionString)
    {
        Input<string>[] values =
        [
            connectionString,
            config.RequireSecret("dataProtectionCertificate"),
            config.RequireSecret("dataProtectionCertificatePassword"),
            config.RequireSecret("stripeSecretKey"),
            config.GetSecret("dataProtectionPreviousCertificates")
                ?? Output.CreateSecret("[]"),
        ];
        return Output.All(values).Apply(items => JsonSerializer.Serialize(new
        {
            ConnectionStrings = new { Licensing = items[0] },
            DataProtection = new
            {
                Certificate = items[1],
                CertificatePassword = items[2],
                PreviousCertificates = ParsePreviousCertificates(
                    items[4],
                    "dataProtectionPreviousCertificates"),
            },
            Stripe = new
            {
                SecretKey = items[3],
                MonthlyPriceId = config.Require("stripeMonthlyPriceId"),
                AnnualPriceId = config.Require("stripeAnnualPriceId"),
            },
            InvitationEmail = new
            {
                FromAddress = config.Require("invitationFromAddress"),
                AcceptanceUrl = $"{hostedOrigin}/invitations/accept",
                AmazonSes = new { Region = Aws.Config.Region ?? "eu-west-1" },
            },
            InfrastructureSmoke = new
            {
                ProcessingDelaySeconds = 720,
            },
        }));
    }

    private static Output<string> BuildConnectionConfiguration(
        Output<string> connectionString)
    {
        return connectionString.Apply(value => JsonSerializer.Serialize(new
        {
            ConnectionStrings = new { Licensing = value },
        }));
    }

    private static Output<string> BuildProvisioningConfiguration(
        Output<string> connectionString,
        Output<string> runtimePassword)
    {
        return Output.Tuple(connectionString, runtimePassword).Apply(values =>
            JsonSerializer.Serialize(new
            {
                ConnectionStrings = new { Licensing = values.Item1 },
                DatabaseRuntimeRole = new
                {
                    Username = DatabaseRuntimeUsername,
                    Password = values.Item2,
                },
            }));
    }

    private static string QuoteConnectionStringValue(string value)
    {
        return $"\"{value.Replace("\"", "\"\"", StringComparison.Ordinal)}\"";
    }

    private static Output<string> BuildConnectionString(
        Output<string> endpoint,
        Output<string> password,
        string username)
    {
        return Output.Tuple(endpoint, password).Apply(values =>
            "Host=" + QuoteConnectionStringValue(values.Item1)
                + $";Port=5432;Database={DatabaseName};Username={username};Password="
                + QuoteConnectionStringValue(values.Item2)
                + ";SSL Mode=Require");
    }

    private static string ReadHostedOrigin(Config config)
    {
        var value = config.Require("hostedOrigin").TrimEnd('/');
        if (!Uri.TryCreate(value, UriKind.Absolute, out var origin)
            || !string.Equals(origin.Scheme, Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase)
            || !string.IsNullOrEmpty(origin.UserInfo)
            || origin.AbsolutePath != "/"
            || !string.IsNullOrEmpty(origin.Query)
            || !string.IsNullOrEmpty(origin.Fragment))
        {
            throw new InvalidOperationException(
                "hostedOrigin must be an absolute HTTPS origin without a path, query, or fragment.");
        }

        return value;
    }

    private static CustomDomainConfiguration? ReadCustomDomain(
        Config config,
        string hostedOrigin)
    {
        var domainName = config.Get("domainName")?.Trim();
        var hostedZoneId = config.Get("hostedZoneId")?.Trim();
        var certificateArn = config.Get("certificateArn")?.Trim();
        if (domainName is null && hostedZoneId is null && certificateArn is null)
        {
            return null;
        }

        if (string.IsNullOrEmpty(domainName)
            || string.IsNullOrEmpty(hostedZoneId)
            || string.IsNullOrEmpty(certificateArn))
        {
            throw new InvalidOperationException(
                "domainName, hostedZoneId, and certificateArn must be configured together.");
        }

        if (!string.Equals(
            hostedOrigin,
            $"https://{domainName}",
            StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "hostedOrigin must match the configured HTTPS custom domain.");
        }

        return new(domainName, hostedZoneId, certificateArn);
    }

    private static int ReadMaximumWorkerTasks(Config config)
    {
        var configured = config.Get("maximumWorkerTasks");
        var maximum = 4;
        if (configured is not null
            && (!configured.All(char.IsAsciiDigit)
                || !int.TryParse(configured, out maximum))
            || maximum is < 1 or > 32)
        {
            throw new InvalidOperationException(
                "maximumWorkerTasks must be between 1 and 32.");
        }

        return maximum;
    }

    private static AuthenticationProviderConfiguration ReadAuthenticationProvider(
        Config config,
        string provider)
    {
        var clientId = config.Get($"{provider}ClientId")?.Trim();
        var secret = config.GetSecret($"{provider}ClientSecret");
        if (string.IsNullOrEmpty(clientId) && secret is null)
        {
            return new(false, null, Output.CreateSecret(string.Empty));
        }

        if (string.IsNullOrEmpty(clientId) || secret is null)
        {
            throw new InvalidOperationException(
                $"The {provider} authentication provider requires both client ID and secret.");
        }

        return new(true, clientId, secret);
    }

    private static PreviousCertificateConfiguration[] ParsePreviousCertificates(
        string value,
        string configurationKey)
    {
        try
        {
            var certificates = JsonSerializer.Deserialize<PreviousCertificateConfiguration[]>(
                value,
                CaseInsensitiveJson) ?? [];
            if (certificates.Any(certificate =>
                string.IsNullOrWhiteSpace(certificate.Certificate)
                || string.IsNullOrWhiteSpace(certificate.CertificatePassword)))
            {
                throw new InvalidOperationException(
                    $"{configurationKey} contains an incomplete certificate entry.");
            }

            return certificates;
        }
        catch (JsonException exception)
        {
            throw new InvalidOperationException(
                $"{configurationKey} must be a JSON array of certificate/password objects.",
                exception);
        }
    }

}
