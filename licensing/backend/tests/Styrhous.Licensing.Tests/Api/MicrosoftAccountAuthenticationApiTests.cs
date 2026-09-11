using System.Net;
using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.AspNetCore.Authentication.OpenIdConnect;
using Microsoft.AspNetCore.TestHost;
using Microsoft.AspNetCore.WebUtilities;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.IdentityModel.JsonWebTokens;
using Microsoft.IdentityModel.Protocols;
using Microsoft.IdentityModel.Protocols.OpenIdConnect;
using Microsoft.IdentityModel.Tokens;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class MicrosoftAccountAuthenticationApiTests
{
    [TestCase("verified_primary_email")]
    [TestCase("verified_secondary_email")]
    [TestCase("xms_edov")]
    [TestCase("organization")]
    public async Task ValidatedMicrosoftClaimsPreserveTheExistingGraphIdentity(string emailEvidence)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var provider = new MicrosoftProvider(emailEvidence);
        using var baseFactory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow,
            useTestAuthentication: false, configureExternalProviders: true);
        using var factory = baseFactory.WithWebHostBuilder(builder => builder.ConfigureTestServices(services =>
            services.PostConfigure<OpenIdConnectOptions>("microsoft", provider.Configure)));
        using var client = factory.CreateClient(new() { BaseAddress = new("https://localhost"), AllowAutoRedirect = false });

        await SignInAsync(client, provider);
        provider.Subject = "a-different-oidc-subject";
        await SignInAsync(client, provider);

        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(context.UserAccounts.Count(), Is.EqualTo(1));
            Assert.That(context.Trials.Count(), Is.EqualTo(1));
            Assert.That(context.ExternalIdentities.Single().Subject, Is.EqualTo("stable-graph-id"));
            Assert.That(context.UserAccounts.Single().VerifiedEmail, Is.EqualTo("verified@example.com"));
            Assert.That(provider.GraphRequests, Is.EqualTo(2));
        });
    }

    [TestCase("missing")]
    [TestCase("false")]
    [TestCase("string-true")]
    [TestCase("bad-signature")]
    [TestCase("unsigned")]
    [TestCase("bad-audience")]
    [TestCase("bad-issuer")]
    [TestCase("bad-tenant")]
    [TestCase("bad-key-issuer")]
    [TestCase("expired")]
    [TestCase("bad-nonce")]
    public async Task RejectedMicrosoftTokensCannotCreateAccountsOrTrials(string fault)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var provider = new MicrosoftProvider(fault);
        using var baseFactory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow,
            useTestAuthentication: false, configureExternalProviders: true);
        using var factory = baseFactory.WithWebHostBuilder(builder => builder.ConfigureTestServices(services =>
            services.PostConfigure<OpenIdConnectOptions>("microsoft", provider.Configure)));
        using var client = factory.CreateClient(new() { BaseAddress = new("https://localhost"), AllowAutoRedirect = false });

        var result = await CallbackAsync(client, provider);
        using (result)
        {
            using var completed = result.Headers.Location?.OriginalString.Contains("/auth/callback/microsoft", StringComparison.Ordinal) == true
                ? await client.GetAsync(result.Headers.Location) : null;
            var redirect = (completed ?? result).Headers.Location!.OriginalString;
            Assert.That(redirect, Does.Contain("authenticationError=" +
                (fault is "missing" or "false" or "string-true"
                    ? "microsoft_verified_email_required" : "provider_authentication_failed")));
        }

        using var sessionResponse = await client.GetAsync("/auth/session");
        using var session = JsonDocument.Parse(await sessionResponse.Content.ReadAsStringAsync());
        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(session.RootElement.GetProperty("authenticated").GetBoolean(), Is.False);
            Assert.That(context.UserAccounts.Count(), Is.Zero);
            Assert.That(context.Trials.Count(), Is.Zero);
            Assert.That(context.ExternalIdentities.Count(), Is.Zero);
            Assert.That(context.VerifiedEmailClaims.Count(), Is.Zero);
        });
    }

    private static async Task SignInAsync(HttpClient client, MicrosoftProvider provider)
    {
        using var callback = await CallbackAsync(client, provider);
        Assert.That(callback.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
        Assert.That(callback.Headers.Location!.OriginalString, Does.Contain("/auth/callback/microsoft"));
        using var completed = await client.GetAsync(callback.Headers.Location);
        Assert.That(completed.Headers.Location!.OriginalString, Is.EqualTo("/account"));
    }

    [TestCase("sign-in")]
    [TestCase("link")]
    public async Task RejectedExistingAccountCallbackDoesNotChangeIdentityOrSession(string operation)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var provider = new MicrosoftProvider("xms_edov");
        using var baseFactory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow,
            useTestAuthentication: false, configureExternalProviders: true);
        using var factory = baseFactory.WithWebHostBuilder(builder => builder.ConfigureTestServices(services =>
            services.PostConfigure<OpenIdConnectOptions>("microsoft", provider.Configure)));
        using var client = factory.CreateClient(new() { BaseAddress = new("https://localhost"), AllowAutoRedirect = false });
        await SignInAsync(client, provider);
        provider.Mode = "missing";
        provider.Email = "unverified-change@example.com";
        using var callback = await CallbackAsync(client, provider, operation);
        using var completed = await client.GetAsync(callback.Headers.Location);
        Assert.That(completed.Headers.Location!.OriginalString, Does.Contain("authenticationError=microsoft_verified_email_required"));
        using var sessionResponse = await client.GetAsync("/auth/session");
        using var session = JsonDocument.Parse(await sessionResponse.Content.ReadAsStringAsync());
        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(session.RootElement.GetProperty("authenticated").GetBoolean(), Is.True);
            Assert.That(context.UserAccounts.Single().VerifiedEmail, Is.EqualTo("verified@example.com"));
            Assert.That(context.VerifiedEmailClaims.Count(), Is.EqualTo(1));
            Assert.That(context.ExternalIdentities.Count(), Is.EqualTo(1));
            Assert.That(context.Trials.Count(), Is.EqualTo(1));
            Assert.That(provider.GraphRequests, Is.EqualTo(1));
        });
    }

    [TestCase("missing-state")]
    [TestCase("tampered-state")]
    [TestCase("missing-correlation")]
    [TestCase("replay")]
    public async Task CallbackRequiresItsOriginalUnconsumedBrowserChallenge(string fault)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var provider = new MicrosoftProvider("xms_edov");
        using var baseFactory = new LicensingWebApplicationFactory(database, DateTimeOffset.UtcNow,
            useTestAuthentication: false, configureExternalProviders: true);
        using var factory = baseFactory.WithWebHostBuilder(builder => builder.ConfigureTestServices(services =>
            services.PostConfigure<OpenIdConnectOptions>("microsoft", provider.Configure)));
        using var client = factory.CreateClient(new() { BaseAddress = new("https://localhost"), AllowAutoRedirect = false });
        using var challenge = await client.GetAsync("/auth/sign-in/microsoft?returnUrl=/account");
        var query = QueryHelpers.ParseQuery(challenge.Headers.Location!.Query);
        provider.Nonce = query["nonce"].ToString();
        var callbackUrl = QueryHelpers.AddQueryString("/auth/provider-callback/microsoft",
            new Dictionary<string, string?>
            {
                ["code"] = "fixture-code",
                ["state"] = fault switch
                { "missing-state" => null, "tampered-state" => "invalid", _ => query["state"].ToString() }
            });
        if (fault == "replay")
        {
            using var first = await client.GetAsync(callbackUrl);
            using var completed = await client.GetAsync(first.Headers.Location);
            Assert.That(completed.Headers.Location!.OriginalString, Is.EqualTo("/account"));
        }
        using var otherBrowser = factory.CreateClient(new() { BaseAddress = new("https://localhost"), AllowAutoRedirect = false });
        using var rejected = await (fault == "missing-correlation" ? otherBrowser : client).GetAsync(callbackUrl);
        Assert.That(rejected.Headers.Location!.OriginalString, Does.Contain("authenticationError=provider_authentication_failed"));
        await using var context = database.CreateContext();
        Assert.That(context.UserAccounts.Count(), Is.EqualTo(fault == "replay" ? 1 : 0));
        Assert.That(provider.GraphRequests, Is.EqualTo(fault == "replay" ? 1 : 0));
    }

    private static async Task<HttpResponseMessage> CallbackAsync(HttpClient client, MicrosoftProvider provider, string operation = "sign-in")
    {
        using var challenge = await client.GetAsync($"/auth/{operation}/microsoft?returnUrl=/account");
        var query = QueryHelpers.ParseQuery(challenge.Headers.Location!.Query);
        Assert.Multiple(() =>
        {
            Assert.That(query["response_type"].ToString(), Is.EqualTo("code"));
            Assert.That(query["code_challenge_method"].ToString(), Is.EqualTo("S256"));
            Assert.That(query["code_challenge"].ToString(), Is.Not.Empty);
        });
        provider.Nonce = query["nonce"].ToString();
        return await client.GetAsync(QueryHelpers.AddQueryString("/auth/provider-callback/microsoft",
            new Dictionary<string, string?> { ["code"] = "fixture-code", ["state"] = query["state"].ToString() }));
    }

    private sealed class MicrosoftProvider(string mode) : HttpMessageHandler
    {
        private const string Tenant = "9188040d-6c67-4c5b-b112-36a304b66dad";
        private readonly RSA _rsa = RSA.Create(2048);
        private readonly RSA _otherRsa = RSA.Create(2048);
        private string _audience = "";
        public string Mode { get; set; } = mode;
        private string TenantId => Mode == "organization" ? "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa" : Tenant;
        public string Nonce { get; set; } = "";
        public string Subject { get; set; } = "oidc-subject";
        public string Email { get; set; } = "verified@example.com";
        public int GraphRequests { get; private set; }

        public void Configure(OpenIdConnectOptions options)
        {
            _audience = options.ClientId!;
            var key = JsonWebKeyConverter.ConvertFromRSASecurityKey(new RsaSecurityKey(_rsa) { KeyId = "test-key" });
            key.AdditionalData["issuer"] = Mode == "bad-key-issuer"
                ? "https://login.microsoftonline.com/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/v2.0"
                : Mode == "organization" ? MicrosoftAccountAuthentication.IssuerTemplate.Replace("{tenantid}", TenantId)
                : MicrosoftAccountAuthentication.IssuerTemplate;
            var configuration = new OpenIdConnectConfiguration
            {
                Issuer = MicrosoftAccountAuthentication.IssuerTemplate,
                AuthorizationEndpoint = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
                TokenEndpoint = "https://provider.invalid/token",
                JsonWebKeySet = new JsonWebKeySet(),
            };
            configuration.JsonWebKeySet.Keys.Add(key);
            foreach (var signingKey in configuration.JsonWebKeySet.GetSigningKeys())
            {
                configuration.SigningKeys.Add(signingKey);
            }
            options.Configuration = configuration;
            options.ConfigurationManager = new StaticConfigurationManager<OpenIdConnectConfiguration>(configuration);
            options.Backchannel = new HttpClient(this, disposeHandler: false);
        }

        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            if (request.RequestUri!.Host == "graph.microsoft.com")
            {
                Assert.That(request.Headers.Authorization?.Parameter, Is.EqualTo("access-from-validated-code"));
                GraphRequests++;
                return Json(new { id = "stable-graph-id", mail = "untrusted@example.com", userPrincipalName = "untrusted@example.com" });
            }
            var form = QueryHelpers.ParseQuery(await request.Content!.ReadAsStringAsync(cancellationToken));
            Assert.That(form["code_verifier"].ToString(), Is.Not.Empty);
            var claims = new Dictionary<string, object>
            {
                ["sub"] = Subject,
                ["tid"] = Mode == "bad-tenant" ? "not-a-guid" : TenantId,
                ["nonce"] = Mode == "bad-nonce" ? "not-the-request-nonce" : Nonce,
                ["email"] = Email,
            };
            if (Mode is "verified_primary_email" or "verified_secondary_email")
                claims[Mode] = "verified@example.com";
            else if (Mode != "missing")
                claims["xms_edov"] = Mode == "string-true" ? "true" : Mode != "false";
            var now = DateTime.UtcNow;
            var jwt = new JsonWebTokenHandler().CreateToken(new SecurityTokenDescriptor
            {
                Issuer = Mode == "bad-issuer" ? "https://attacker.invalid/" : MicrosoftAccountAuthentication.IssuerTemplate.Replace("{tenantid}", TenantId),
                Audience = Mode == "bad-audience" ? "other-application" : _audience,
                IssuedAt = now.AddHours(-2),
                NotBefore = now.AddHours(-2),
                Expires = Mode == "expired" ? now.AddHours(-1) : now.AddMinutes(10),
                Claims = claims,
                AdditionalHeaderClaims = Mode == "unsigned" ? new Dictionary<string, object> { ["kid"] = "test-key" } : null,
                SigningCredentials = Mode == "unsigned" ? null : new SigningCredentials(new RsaSecurityKey(Mode == "bad-signature" ? _otherRsa : _rsa) { KeyId = "test-key" }, SecurityAlgorithms.RsaSha256),
            });
            return Json(new { token_type = "Bearer", access_token = "access-from-validated-code", id_token = jwt, expires_in = 600 });
        }

        private static HttpResponseMessage Json(object value)
        {
            return new(HttpStatusCode.OK)
            {
                Content = new StringContent(JsonSerializer.Serialize(value), System.Text.Encoding.UTF8, "application/json"),
            };
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) { _rsa.Dispose(); _otherRsa.Dispose(); }
            base.Dispose(disposing);
        }
    }
}
