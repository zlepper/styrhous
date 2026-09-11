using Styrhous.Licensing.Api.Authentication;

namespace Styrhous.Licensing.Runtime;

internal sealed class BrowserAuthenticationStartupValidation(
    IHostEnvironment environment, IConfiguration configuration) : IHostedService
{
    public Task StartAsync(CancellationToken cancellationToken)
    {
        if (environment.IsProduction())
        {
            ValidateAuthenticationProviders();
        }
        return Task.CompletedTask;
    }

    public Task StopAsync(CancellationToken cancellationToken)
    {
        return Task.CompletedTask;
    }

    private void ValidateAuthenticationProviders()
    {
        var hasCompleteProvider = false;
        foreach (var provider in AccountAuthenticationProviders.All)
        {
            var prefix = $"Authentication:{provider.ConfigurationSection}";
            var hasClientId = !string.IsNullOrWhiteSpace(
                configuration[$"{prefix}:ClientId"]);
            var hasClientSecret = !string.IsNullOrWhiteSpace(
                configuration[$"{prefix}:ClientSecret"]);
            if (hasClientId != hasClientSecret)
            {
                throw new InvalidOperationException(
                    $"{prefix} requires both ClientId and ClientSecret.");
            }

            hasCompleteProvider |= hasClientId;
        }

        if (!hasCompleteProvider)
        {
            throw new InvalidOperationException(
                "Production requires at least one configured Authentication provider.");
        }
    }
}
