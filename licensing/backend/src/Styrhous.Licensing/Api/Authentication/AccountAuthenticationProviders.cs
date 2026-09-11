namespace Styrhous.Licensing.Api.Authentication;

internal static class AccountAuthenticationProviders
{
    public const string GitHub = "github";
    public const string Google = "google";
    public const string Microsoft = "microsoft";
    public const string GitHubConfiguration = "GitHub";
    public const string GoogleConfiguration = "Google";
    public const string MicrosoftConfiguration = "Microsoft";

    public static IReadOnlyList<AccountAuthenticationProvider> All { get; } =
        Array.AsReadOnly<AccountAuthenticationProvider>(
        [
            new(GitHub, GitHubConfiguration),
            new(Google, GoogleConfiguration),
            new(Microsoft, MicrosoftConfiguration),
        ]);

    public static bool IsSupported(string provider)
    {
        return All.Any(candidate => candidate.Scheme == provider);
    }
}

internal readonly record struct AccountAuthenticationProvider(
    string Scheme,
    string ConfigurationSection);
