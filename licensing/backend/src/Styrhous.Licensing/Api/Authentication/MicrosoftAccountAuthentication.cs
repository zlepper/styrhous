using System.Net.Http.Headers;
using System.Security.Claims;
using System.Text.Json;
using Microsoft.AspNetCore.Authentication.OpenIdConnect;
using Microsoft.IdentityModel.JsonWebTokens;
using Microsoft.IdentityModel.Protocols.OpenIdConnect;
using Microsoft.IdentityModel.Tokens;

namespace Styrhous.Licensing.Api.Authentication;

internal static class MicrosoftAccountAuthentication
{
    internal const string IssuerTemplate = "https://login.microsoftonline.com/{tenantid}/v2.0";

    internal static void Configure(OpenIdConnectOptions options, string clientId, string clientSecret)
    {
        options.SignInScheme = AccountAuthentication.ExternalScheme;
        options.CallbackPath = "/auth/provider-callback/microsoft";
        options.Authority = "https://login.microsoftonline.com/common/v2.0";
        options.ClientId = clientId;
        options.ClientSecret = clientSecret;
        options.ResponseType = OpenIdConnectResponseType.Code;
        options.UsePkce = true;
        options.MapInboundClaims = false;
        options.TokenValidationParameters.ValidAlgorithms = [SecurityAlgorithms.RsaSha256];
        options.Scope.Add("email");
        options.Scope.Add("https://graph.microsoft.com/User.Read");
        options.TokenValidationParameters.IssuerValidator = ValidateIssuer;
        options.Events.OnTokenValidated = ValidateAccountAsync;
    }

    private static string ValidateIssuer(string issuer, SecurityToken token, TokenValidationParameters parameters)
    {
        if (token is not JsonWebToken jwt
            || !jwt.TryGetPayloadValue<string>("tid", out var tenantId)
            || !Guid.TryParseExact(tenantId, "D", out _)
            || issuer != IssuerTemplate.Replace("{tenantid}", tenantId, StringComparison.Ordinal))
        {
            throw new SecurityTokenInvalidIssuerException("The Microsoft token issuer does not match its tenant.");
        }

        return issuer;
    }

    private static async Task ValidateAccountAsync(TokenValidatedContext context)
    {
        // Microsoft's common metadata also scopes individual signing keys to issuers.
        var configuration = context.Options.Configuration
            ?? await context.Options.ConfigurationManager!.GetConfigurationAsync(context.HttpContext.RequestAborted);
        var jwt = context.SecurityToken;
        // The framework permits unsigned ID tokens delivered over its code-flow
        // backchannel. Our email trust policy additionally requires a signature.
        if (jwt.Header.Alg != SecurityAlgorithms.RsaSha256 || string.IsNullOrEmpty(jwt.RawSignature))
        {
            context.Fail("The Microsoft ID token must be signed.");
            return;
        }
        var key = configuration.JsonWebKeySet?.Keys.SingleOrDefault(key => key.Kid == jwt.Header.Kid);
        if (key is null
            || !key.AdditionalData.TryGetValue("issuer", out var keyIssuer)
            || keyIssuer.ToString()?.Replace("{tenantid}", jwt.Claims.Single(claim => claim.Type == "tid").Value, StringComparison.Ordinal)
                != jwt.Issuer)
        {
            context.Fail("The Microsoft signing key does not authorize this issuer.");
            return;
        }

        var email = GetVerifiedEmail(context.Principal!);
        var identity = new ClaimsIdentity(context.Scheme.Name);
        if (email is not null)
        {
            // Preserve the Graph subject used by existing Microsoft OAuth logins. Never
            // substitute OIDC sub/oid: personal Microsoft accounts use a different Graph id.
            using var request = new HttpRequestMessage(HttpMethod.Get, "https://graph.microsoft.com/v1.0/me?$select=id");
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", context.TokenEndpointResponse!.AccessToken);
            using var response = await context.Options.Backchannel.SendAsync(request, context.HttpContext.RequestAborted);
            response.EnsureSuccessStatusCode();
            using var profile = JsonDocument.Parse(await response.Content.ReadAsStreamAsync(context.HttpContext.RequestAborted));
            var subject = profile.RootElement.GetProperty("id").GetString();
            if (string.IsNullOrWhiteSpace(subject))
            {
                context.Fail("The Microsoft account identifier is missing.");
                return;
            }

            identity.AddClaim(new Claim(ClaimTypes.NameIdentifier, subject));
            identity.AddClaim(new Claim(AccountAuthentication.VerifiedEmailClaim, email));
        }

        // An empty external identity reaches the shared verified-email rejection path;
        // no account, email claim, trial, link, or application session is created.
        context.Principal = new ClaimsPrincipal(identity);
    }

    internal static string? GetVerifiedEmail(ClaimsPrincipal principal)
    {
        var authoritative = principal.FindAll("verified_primary_email")
            .Concat(principal.FindAll("verified_secondary_email"))
            .Where(claim => claim.ValueType == ClaimValueTypes.String)
            .Select(claim => claim.Value.Trim())
            .FirstOrDefault(email => !string.IsNullOrWhiteSpace(email));
        return authoritative ?? (principal.HasClaim(claim => claim.Type == "xms_edov"
            && claim.ValueType == ClaimValueTypes.Boolean
            && bool.TryParse(claim.Value, out var verified) && verified)
                ? principal.FindFirst(claim => claim.Type == "email" && claim.ValueType == ClaimValueTypes.String)?.Value.Trim() : null);
    }
}
