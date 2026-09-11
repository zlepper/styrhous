using System.Net.Http.Headers;
using System.Security.Claims;
using System.Text.Json;
using AspNet.Security.OAuth.GitHub;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Authentication.Google;
using Microsoft.AspNetCore.Authentication.OAuth;
using Microsoft.AspNetCore.WebUtilities;

namespace Styrhous.Licensing.Api.Authentication;

internal static class AccountAuthentication
{
    public const string SessionScheme = "Styrhous.Session";
    public const string ExternalScheme = "Styrhous.External";
    public const string VerifiedEmailClaim = "styrhous:verified_email";
    public const string ReturnUrlProperty = "styrhous:return_url";
    public const string ErrorQueryParameter = "authenticationError";
    internal const string OperationProperty = "styrhous:operation";
    internal const string ProviderProperty = "styrhous:provider";
    internal const string LinkUserProperty = "styrhous:link_user";
    internal const string SignInOperation = "sign-in";
    internal const string LinkOperation = "link";
    internal const string ReauthenticationOperation = "reauth";

    public static void ConfigureProviders(
        AuthenticationBuilder authentication,
        IConfiguration configuration)
    {
        var section = configuration.GetSection("Authentication");
        if (TryCredentials(
            section,
            AccountAuthenticationProviders.GitHubConfiguration,
            out var github))
        {
            authentication.AddGitHub(AccountAuthenticationProviders.GitHub, options =>
            {
                options.SignInScheme = ExternalScheme;
                options.CallbackPath = "/auth/provider-callback/github";
                options.ClientId = github.ClientId;
                options.ClientSecret = github.ClientSecret;
                options.Scope.Add("user:email");
                options.Events.OnCreatingTicket = AddGitHubVerifiedEmailAsync;
                options.Events.OnRemoteFailure = RedirectRemoteFailureAsync;
            });
        }

        if (TryCredentials(
            section,
            AccountAuthenticationProviders.GoogleConfiguration,
            out var google))
        {
            authentication.AddGoogle(AccountAuthenticationProviders.Google, options =>
            {
                options.SignInScheme = ExternalScheme;
                options.CallbackPath = "/auth/provider-callback/google";
                options.ClientId = google.ClientId;
                options.ClientSecret = google.ClientSecret;
                options.Events.OnCreatingTicket = context =>
                {
                    AddVerifiedEmail(
                        context.Identity,
                        GetGoogleVerifiedEmail(context.User));

                    return Task.CompletedTask;
                };
                options.Events.OnRemoteFailure = RedirectRemoteFailureAsync;
            });
        }

        if (TryCredentials(
            section,
            AccountAuthenticationProviders.MicrosoftConfiguration,
            out var microsoft))
        {
            authentication.AddOpenIdConnect(
                AccountAuthenticationProviders.Microsoft,
                options =>
            {
                MicrosoftAccountAuthentication.Configure(options, microsoft.ClientId, microsoft.ClientSecret);
                options.Events.OnRemoteFailure = RedirectRemoteFailureAsync;
            });
        }
    }

    internal static string SafeReturnUrl(string? returnUrl)
    {
        return !string.IsNullOrWhiteSpace(returnUrl)
            && returnUrl[0] == '/'
            && (returnUrl.Length == 1 || returnUrl[1] != '/')
            && !returnUrl.Contains('\\')
                ? returnUrl
                : "/";
    }

    internal static IResult FailureRedirect(string? returnUrl, string reasonCode)
    {
        return Results.Redirect(QueryHelpers.AddQueryString(
            SafeReturnUrl(returnUrl),
            ErrorQueryParameter,
            reasonCode));
    }

    private static async Task RedirectRemoteFailureAsync(RemoteFailureContext context)
    {
        string? returnUrl = null;
        context.Properties?.Items.TryGetValue(ReturnUrlProperty, out returnUrl);
        await context.HttpContext.SignOutAsync(ExternalScheme);
        context.Response.Redirect(QueryHelpers.AddQueryString(
            SafeReturnUrl(returnUrl),
            ErrorQueryParameter,
            "provider_authentication_failed"));
        context.HandleResponse();
    }

    private static bool TryCredentials(
        IConfiguration configuration,
        string provider,
        out ProviderCredentials credentials)
    {
        var clientId = configuration[$"{provider}:ClientId"]?.Trim();
        var clientSecret = configuration[$"{provider}:ClientSecret"]?.Trim();
        if (string.IsNullOrEmpty(clientId) || string.IsNullOrEmpty(clientSecret))
        {
            credentials = default;
            return false;
        }

        credentials = new ProviderCredentials(clientId, clientSecret);
        return true;
    }

    private static async Task AddGitHubVerifiedEmailAsync(
        OAuthCreatingTicketContext context)
    {
        using var request = new HttpRequestMessage(
            HttpMethod.Get,
            "https://api.github.com/user/emails");
        request.Headers.Authorization = new AuthenticationHeaderValue(
            "Bearer",
            context.AccessToken);
        request.Headers.Accept.ParseAdd("application/vnd.github+json");
        request.Headers.UserAgent.ParseAdd("Styrhous-Licensing");
        using var response = await context.Backchannel.SendAsync(
            request,
            context.HttpContext.RequestAborted);
        response.EnsureSuccessStatusCode();
        using var document = JsonDocument.Parse(
            await response.Content.ReadAsStreamAsync(context.HttpContext.RequestAborted));
        var verifiedEmail = GetGitHubVerifiedEmail(document.RootElement);
        AddVerifiedEmail(context.Identity, verifiedEmail);
    }

    internal static string? GetGitHubVerifiedEmail(JsonElement emails)
    {
        return emails.ValueKind is JsonValueKind.Array
            ? emails.EnumerateArray()
                .Where(item => GetBoolean(item, "verified"))
                .OrderByDescending(item => GetBoolean(item, "primary"))
                .Select(item => GetString(item, "email"))
                .FirstOrDefault(email => !string.IsNullOrWhiteSpace(email))
            : null;
    }

    internal static string? GetGoogleVerifiedEmail(JsonElement user)
    {
        return GetBoolean(user, "email_verified")
            ? GetString(user, "email")
            : null;
    }

    private static bool GetBoolean(JsonElement element, string property)
    {
        return element.ValueKind is JsonValueKind.Object
        && element.TryGetProperty(property, out var value)
        && value.ValueKind is JsonValueKind.True;
    }

    private static string? GetString(JsonElement element, string property)
    {
        return element.ValueKind is JsonValueKind.Object
            && element.TryGetProperty(property, out var value)
            && value.ValueKind is JsonValueKind.String
                ? value.GetString()
                : null;
    }

    private static void AddVerifiedEmail(ClaimsIdentity? identity, string? email)
    {
        if (identity is not null && !string.IsNullOrWhiteSpace(email))
        {
            identity.AddClaim(new Claim(VerifiedEmailClaim, email.Trim()));
        }
    }

    private readonly record struct ProviderCredentials(string ClientId, string ClientSecret);
}
