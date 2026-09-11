using Amazon.SecretsManager;
using Amazon.SecretsManager.Model;

namespace Styrhous.Licensing.Runtime;

internal static class ApplicationConfigurationLoader
{
    internal const string InlineConfigurationVariable =
        "STYRHOUS_APPLICATION_CONFIGURATION";
    internal const string SecretArnVariable = "LICENSING_SECRET_ARN";
    internal const string SecretVersionVariable = "LICENSING_SECRET_VERSION";

    internal static string? ReadInline()
    {
        return NonEmpty(Environment.GetEnvironmentVariable(InlineConfigurationVariable));
    }

    internal static Task<string?> LoadAsync(
        CancellationToken cancellationToken = default)
    {
        return LoadAsync(
            Environment.GetEnvironmentVariable,
            static () => new AwsApplicationSecretReader(),
            cancellationToken);
    }

    internal static async Task<string?> LoadAsync(
        Func<string, string?> readEnvironment,
        Func<IApplicationSecretReader> createSecretReader,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(readEnvironment);
        ArgumentNullException.ThrowIfNull(createSecretReader);

        var inline = NonEmpty(readEnvironment(InlineConfigurationVariable));
        var secretArn = NonEmpty(readEnvironment(SecretArnVariable))?.Trim();
        var secretVersion = NonEmpty(readEnvironment(SecretVersionVariable))?.Trim();
        if (inline is not null && (secretArn is not null || secretVersion is not null))
        {
            throw new InvalidOperationException(
                $"Configure {InlineConfigurationVariable} or the licensing secret reference, not both.");
        }

        if (inline is not null)
        {
            return inline;
        }

        if (secretArn is null)
        {
            if (secretVersion is not null)
            {
                throw new InvalidOperationException(
                    $"{SecretVersionVariable} requires {SecretArnVariable}.");
            }

            return null;
        }

        using var reader = createSecretReader();
        var secret = NonEmpty(
            await reader.ReadSecretStringAsync(
                secretArn,
                secretVersion,
                cancellationToken)) ?? throw new InvalidOperationException(
                "The licensing application secret must contain a non-empty SecretString value.");
        return secret;
    }

    private static string? NonEmpty(string? value)
    {
        return string.IsNullOrWhiteSpace(value) ? null : value;
    }
}

internal interface IApplicationSecretReader : IDisposable
{
    Task<string?> ReadSecretStringAsync(
        string secretId,
        string? versionId,
        CancellationToken cancellationToken);
}

internal sealed class AwsApplicationSecretReader : IApplicationSecretReader
{
    private readonly IAmazonSecretsManager _client;

    internal AwsApplicationSecretReader()
        : this(new AmazonSecretsManagerClient())
    {
    }

    internal AwsApplicationSecretReader(IAmazonSecretsManager client)
    {
        ArgumentNullException.ThrowIfNull(client);
        _client = client;
    }

    public async Task<string?> ReadSecretStringAsync(
        string secretId,
        string? versionId,
        CancellationToken cancellationToken)
    {
        var response = await _client.GetSecretValueAsync(
            new GetSecretValueRequest
            {
                SecretId = secretId,
                VersionId = versionId,
            },
            cancellationToken);
        return response.SecretString;
    }

    public void Dispose()
    {
        _client.Dispose();
    }
}
