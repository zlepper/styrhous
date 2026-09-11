using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.WebUtilities;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class AccountAuthenticationApiTests
{
    private static readonly string?[] GitHubAndGoogle = ["github", "google"];
    private static readonly string?[] GitHubOnly = ["github"];

    [Test]
    public async Task ExternalProviderUsesTheCanonicalSameOriginCallback()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            desktopIssuer: "https://licenses.example.com/",
            configureExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var response = await client.GetAsync("/auth/sign-in/github?returnUrl=/account");
        var authorization = response.Headers.Location
            ?? throw new AssertionException("The provider challenge did not redirect.");
        var query = QueryHelpers.ParseQuery(authorization.Query);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(authorization.Host, Is.EqualTo("github.com"));
            Assert.That(
                query["redirect_uri"].SingleOrDefault(),
                Is.EqualTo(
                    "https://licenses.example.com/auth/provider-callback/github"));
        });
    }

    [Test]
    public async Task ExternalSignInAndExplicitLinkCreateOneCookieSessionAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github?returnUrl=/account");
        using var completedSignIn = await client.GetAsync(signIn.Headers.Location);
        using var link = await client.GetAsync("/auth/link/google?returnUrl=/account");
        using var completedLink = await client.GetAsync(link.Headers.Location);
        using var sessionResponse = await client.GetAsync("/auth/session");
        using var session = JsonDocument.Parse(await sessionResponse.Content.ReadAsStringAsync());
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(signIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(signIn.Headers.Location?.OriginalString, Is.EqualTo("/auth/callback/github"));
            Assert.That(completedSignIn.Headers.Location?.OriginalString, Is.EqualTo("/account"));
            Assert.That(link.Headers.Location?.OriginalString, Is.EqualTo("/auth/callback/google"));
            Assert.That(completedLink.Headers.Location?.OriginalString, Is.EqualTo("/account"));
            Assert.That(session.RootElement.GetProperty("authenticated").GetBoolean(), Is.True);
            Assert.That(
                session.RootElement.GetProperty("email").GetString(),
                Is.EqualTo("external@example.com"));
            Assert.That(
                session.RootElement.GetProperty("linkedProviders")
                    .EnumerateArray()
                    .Select(provider => provider.GetString()),
                Is.EqualTo(GitHubAndGoogle));
            Assert.That(
                session.RootElement.GetProperty("recentlyAuthenticated").GetBoolean(),
                Is.True);
        });
    }

    [Test]
    public async Task ConcurrentExternalCallbacksForTheSameIdentityConvergeOnOneAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        using var firstFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true,
            interceptors: [new SignupPreflightBarrierInterceptor(barrier)]);
        using var secondFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true,
            interceptors: [new SignupPreflightBarrierInterceptor(barrier)]);
        using var firstClient = firstFactory.CreateApiClient();
        using var secondClient = secondFactory.CreateApiClient();
        using var firstChallenge = await firstClient.GetAsync("/auth/sign-in/github");
        using var secondChallenge = await secondClient.GetAsync("/auth/sign-in/github");

        var completions = await Task.WhenAll(
            firstClient.GetAsync(firstChallenge.Headers.Location),
            secondClient.GetAsync(secondChallenge.Headers.Location));
        try
        {
            await using var context = database.CreateContext();
            Assert.Multiple(() =>
            {
                Assert.That(completions, Has.All.Property("StatusCode").EqualTo(HttpStatusCode.Redirect));
                Assert.That(completions.Select(AuthenticationError), Has.All.Null);
                Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
                Assert.That(context.UserAccounts.Count(), Is.EqualTo(1));
                Assert.That(context.ExternalIdentities.Count(), Is.EqualTo(1));
                Assert.That(context.BillingAccounts.Count(), Is.EqualTo(1));
                Assert.That(context.Seats.Count(), Is.EqualTo(1));
                Assert.That(context.Trials.Count(), Is.EqualTo(1));
            });
        }
        finally
        {
            foreach (var completion in completions)
            {
                completion.Dispose();
            }
        }
    }

    [Test]
    public async Task ReauthenticationRequiresTheSameLinkedProviderIdentity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github?returnUrl=/account");
        using var completedSignIn = await client.GetAsync(signIn.Headers.Location);
        client.DefaultRequestHeaders.Add(
            LicensingWebApplicationFactory.ExternalSubjectHeader,
            "different-github-subject");
        using var reauthentication = await client.GetAsync(
            "/auth/reauth/github?returnUrl=/account");
        using var completedReauthentication = await client.GetAsync(
            reauthentication.Headers.Location);

        Assert.Multiple(() =>
        {
            Assert.That(completedSignIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(reauthentication.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                completedReauthentication.StatusCode,
                Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                AuthenticationError(completedReauthentication),
                Is.EqualTo("provider_not_linked"));
            Assert.That(
                RedirectPath(completedReauthentication),
                Is.EqualTo("/account"));
        });
    }

    [TestCase("missing-subject")]
    [TestCase("missing-verified-email")]
    public async Task ExternalCallbackRejectsAnIncompleteVerifiedIdentity(
        string claimsMode)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();
        client.DefaultRequestHeaders.Add(
            LicensingWebApplicationFactory.ExternalClaimsModeHeader,
            claimsMode);

        using var signIn = await client.GetAsync("/auth/sign-in/github");
        using var completed = await client.GetAsync(signIn.Headers.Location);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(completed.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                AuthenticationError(completed),
                Is.EqualTo("verified_email_required"));
            Assert.That(context.UserAccounts.Count(), Is.Zero);
        });
    }

    [Test]
    public async Task ExternalCallbackRejectsAMalformedVerifiedEmail()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();
        client.DefaultRequestHeaders.Add(
            LicensingWebApplicationFactory.ExternalClaimsModeHeader,
            "malformed-verified-email");

        using var signIn = await client.GetAsync("/auth/sign-in/github");
        using var completed = await client.GetAsync(signIn.Headers.Location);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(completed.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(AuthenticationError(completed), Is.EqualTo("invalid_external_identity"));
            Assert.That(context.UserAccounts.Count(), Is.Zero);
        });
    }

    [Test]
    public async Task ChangedProviderEmailUpdatesTheDomainAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github");
        using var completedSignIn = await client.GetAsync(signIn.Headers.Location);
        client.DefaultRequestHeaders.Add(
            LicensingWebApplicationFactory.ExternalEmailHeader,
            "changed@example.com");
        using var changedSignIn = await client.GetAsync("/auth/sign-in/github");
        using var completedChangedSignIn = await client.GetAsync(
            changedSignIn.Headers.Location);
        await using var context = database.CreateContext();
        var user = await context.UserAccounts.SingleAsync();

        Assert.Multiple(() =>
        {
            Assert.That(completedSignIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completedChangedSignIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(user.VerifiedEmail, Is.EqualTo("changed@example.com"));
        });
    }

    [Test]
    public async Task ExternalCallbackRejectsAnOversizedProviderIdentity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();
        client.DefaultRequestHeaders.Add(
            LicensingWebApplicationFactory.ExternalClaimsModeHeader,
            "oversized-subject");

        using var signIn = await client.GetAsync("/auth/sign-in/github?returnUrl=/account");
        using var completed = await client.GetAsync(signIn.Headers.Location);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(completed.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(AuthenticationError(completed), Is.EqualTo("invalid_external_identity"));
            Assert.That(context.UserAccounts.Count(), Is.Zero);
        });
    }

    [Test]
    public async Task ExternalCallbackRejectsAChangedProvider()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github");
        using var completed = await client.GetAsync("/auth/callback/google");
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(signIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completed.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                AuthenticationError(completed),
                Is.EqualTo("authentication_provider_changed"));
            Assert.That(context.UserAccounts.Count(), Is.Zero);
        });
    }

    [Test]
    public async Task ExternalCallbackRejectsAnUnknownAuthenticationOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();
        client.DefaultRequestHeaders.Add(
            LicensingWebApplicationFactory.ExternalOperationHeader,
            "unexpected-operation");

        using var signIn = await client.GetAsync(
            "/auth/sign-in/github?returnUrl=/account");
        using var completed = await client.GetAsync(signIn.Headers.Location);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(completed.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                AuthenticationError(completed),
                Is.EqualTo("authentication_operation_invalid"));
            Assert.That(context.UserAccounts.Count(), Is.Zero);
        });
    }

    [Test]
    public async Task ExistingVerifiedEmailConflictReturnsToThePortalForExplicitLinking()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var firstClient = factory.CreateApiClient();
        using var secondClient = factory.CreateApiClient();

        using var firstChallenge = await firstClient.GetAsync("/auth/sign-in/github");
        using var firstCompletion = await firstClient.GetAsync(
            firstChallenge.Headers.Location);
        using var secondChallenge = await secondClient.GetAsync(
            "/auth/sign-in/google?returnUrl=/account");
        using var secondCompletion = await secondClient.GetAsync(
            secondChallenge.Headers.Location);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(firstCompletion.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(secondCompletion.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                RedirectPath(secondCompletion),
                Is.EqualTo("/account"));
            Assert.That(
                AuthenticationError(secondCompletion),
                Is.EqualTo("account_link_required"));
            Assert.That(context.UserAccounts.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ProviderRemoteFailureReturnsToTheSafePortalLocation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            configureExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var challenge = await client.GetAsync(
            "/auth/sign-in/github?returnUrl=/account");
        var providerLocation = challenge.Headers.Location
            ?? throw new AssertionException("The provider challenge did not redirect.");
        var state = QueryHelpers.ParseQuery(providerLocation.Query)["state"].Single()
            ?? throw new AssertionException("The provider challenge omitted its state.");
        using var callback = await client.GetAsync(
            "/auth/provider-callback/github?error=access_denied&state="
                + Uri.EscapeDataString(state));

        Assert.Multiple(() =>
        {
            Assert.That(callback.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(RedirectPath(callback), Is.EqualTo("/account"));
            Assert.That(
                AuthenticationError(callback),
                Is.EqualTo("provider_authentication_failed"));
            Assert.That(
                callback.Headers.GetValues("Set-Cookie"),
                Has.Some.StartsWith("__Host-styrhous-external=;"));
        });
    }

    [Test]
    public async Task ExternalSignupRetriesAsOneDomainAccountTransaction()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var interceptor = new ConcurrencyFailureInterceptor(attempt => attempt == 1);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient();

        using var challenge = await client.GetAsync("/auth/sign-in/github");
        using var completion = await client.GetAsync(challenge.Headers.Location);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(completion.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(AuthenticationError(completion), Is.Null);
            Assert.That(interceptor.AttemptCount, Is.GreaterThan(1));
            Assert.That(interceptor.ContextCount, Is.EqualTo(2));
            Assert.That(context.UserAccounts.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ExhaustedExternalSignupRetriesReturnSafelyWithoutPartialAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var interceptor = new ConcurrencyFailureInterceptor(_ => true);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient();

        using var challenge = await client.GetAsync(
            "/auth/sign-in/github?returnUrl=/account");
        using var completion = await client.GetAsync(challenge.Headers.Location);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(completion.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(AuthenticationError(completion), Is.EqualTo("concurrent_modification"));
            Assert.That(interceptor.AttemptCount, Is.EqualTo(3));
            Assert.That(context.UserAccounts.Count(), Is.Zero);
        });
    }

    [Test]
    public async Task ExternalLinkRollsBackWhenProviderPersistenceKeepsFailing()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var firstFailureAttempt = int.MaxValue;
        var interceptor = new ConcurrencyFailureInterceptor(
            attempt => attempt >= firstFailureAttempt);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github");
        using var completedSignIn = await client.GetAsync(signIn.Headers.Location);
        firstFailureAttempt = interceptor.AttemptCount + 1;
        using var link = await client.GetAsync("/auth/link/google?returnUrl=/account");
        using var completedLink = await client.GetAsync(link.Headers.Location);
        await using var context = database.CreateContext();
        var domainProviders = await context.ExternalIdentities
            .Select(identity => identity.Provider)
            .ToArrayAsync();

        Assert.Multiple(() =>
        {
            Assert.That(completedSignIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completedLink.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(AuthenticationError(completedLink), Is.EqualTo("concurrent_modification"));
            Assert.That(interceptor.AttemptCount, Is.EqualTo(firstFailureAttempt + 2));
            Assert.That(domainProviders, Is.EqualTo(GitHubOnly));
        });
    }

    [Test]
    public async Task ExternalUnlinkRollsBackWhenProviderRemovalKeepsFailing()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var firstFailureAttempt = int.MaxValue;
        var interceptor = new ConcurrencyFailureInterceptor(
            attempt => attempt >= firstFailureAttempt);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github");
        using var completedSignIn = await client.GetAsync(signIn.Headers.Location);
        using var link = await client.GetAsync("/auth/link/google");
        using var completedLink = await client.GetAsync(link.Headers.Location);
        using var reauthenticate = await client.GetAsync("/auth/reauth/github");
        using var completedReauthentication = await client.GetAsync(
            reauthenticate.Headers.Location);
        firstFailureAttempt = interceptor.AttemptCount + 1;
        var unlinkRequest = new HttpRequestMessage(
            HttpMethod.Delete,
            "/auth/providers/google");
        AntiforgeryTestClient.AddToken(
            unlinkRequest,
            await AntiforgeryTestClient.GetTokenAsync(client));
        using var unlink = await client.SendAsync(unlinkRequest);
        using var body = JsonDocument.Parse(await unlink.Content.ReadAsStringAsync());
        await using var context = database.CreateContext();
        var domainProviders = await context.ExternalIdentities
            .OrderBy(identity => identity.Provider)
            .Select(identity => identity.Provider)
            .ToArrayAsync();

        Assert.Multiple(() =>
        {
            Assert.That(completedSignIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completedLink.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                completedReauthentication.StatusCode,
                Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(unlink.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("concurrent_modification"));
            Assert.That(interceptor.AttemptCount, Is.EqualTo(firstFailureAttempt + 2));
            Assert.That(domainProviders, Is.EqualTo(GitHubAndGoogle));
        });
    }

    [Test]
    public async Task ReauthenticationAllowsUnlinkAndSignOutClearsTheSessionCookie()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github?returnUrl=/account");
        using var completedSignIn = await client.GetAsync(signIn.Headers.Location);
        using var link = await client.GetAsync("/auth/link/google?returnUrl=/account");
        using var completedLink = await client.GetAsync(link.Headers.Location);
        using var reauthenticate = await client.GetAsync(
            "/auth/reauth/github?returnUrl=/account");
        using var completedReauthentication = await client.GetAsync(
            reauthenticate.Headers.Location);

        var unlinkRequest = new HttpRequestMessage(HttpMethod.Delete, "/auth/providers/google");
        AntiforgeryTestClient.AddToken(
            unlinkRequest,
            await AntiforgeryTestClient.GetTokenAsync(client));
        using var unlink = await client.SendAsync(unlinkRequest);

        var signOutRequest = new HttpRequestMessage(HttpMethod.Post, "/auth/sign-out");
        AntiforgeryTestClient.AddToken(
            signOutRequest,
            await AntiforgeryTestClient.GetTokenAsync(client));
        using var signOut = await client.SendAsync(signOutRequest);
        using var sessionResponse = await client.GetAsync("/auth/session");
        using var session = JsonDocument.Parse(await sessionResponse.Content.ReadAsStringAsync());
        var deletedCookies = signOut.Headers.GetValues("Set-Cookie").ToArray();
        await using var context = database.CreateContext();
        var remainingProviders = await context.ExternalIdentities
            .Select(identity => identity.Provider)
            .ToArrayAsync();

        Assert.Multiple(() =>
        {
            Assert.That(completedSignIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completedLink.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                completedReauthentication.StatusCode,
                Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(unlink.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(signOut.StatusCode, Is.EqualTo(HttpStatusCode.NoContent));
            Assert.That(
                deletedCookies,
                Has.Some.StartsWith("__Host-styrhous-session=;"));
            Assert.That(session.RootElement.GetProperty("authenticated").GetBoolean(), Is.False);
            Assert.That(remainingProviders, Is.EqualTo(GitHubOnly));
        });
    }

    [Test]
    public async Task LinkRequiresAuthenticationWithinTheRecentVerificationWindow()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        string sessionCookie;
        using (var signInFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true))
        using (var signInClient = signInFactory.CreateApiClient())
        {
            using var signIn = await signInClient.GetAsync("/auth/sign-in/github");
            using var completedSignIn = await signInClient.GetAsync(signIn.Headers.Location);
            sessionCookie = SessionCookie(completedSignIn);
        }

        using var expiredFactory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddMinutes(11),
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var expiredClient = expiredFactory.CreateApiClient();
        expiredClient.DefaultRequestHeaders.Add("Cookie", sessionCookie);
        using var link = await expiredClient.GetAsync("/auth/link/google");
        using var body = JsonDocument.Parse(await link.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(link.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("recent_authentication_required"));
        });
    }

    [Test]
    public async Task LinkCallbackRejectsAMissingOriginatingSession()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync("/auth/sign-in/github");
        using var completedSignIn = await client.GetAsync(signIn.Headers.Location);
        using var link = await client.GetAsync("/auth/link/google?returnUrl=/account");
        using var callbackClient = factory.CreateApiClient();
        callbackClient.DefaultRequestHeaders.Add(
            "Cookie",
            Cookie(link, "__Host-styrhous-external"));
        using var completedLink = await callbackClient.GetAsync(link.Headers.Location);
        await using var context = database.CreateContext();
        var domainProviders = await context.ExternalIdentities
            .Select(identity => identity.Provider)
            .ToArrayAsync();

        Assert.Multiple(() =>
        {
            Assert.That(completedSignIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completedLink.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(
                AuthenticationError(completedLink),
                Is.EqualTo("authentication_session_changed"));
            Assert.That(domainProviders, Is.EqualTo(GitHubOnly));
        });
    }

    [Test]
    public async Task AnonymousSessionAndProviderDiscoveryAreStableWithoutConfiguredProviders()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient();

        using var providersResponse = await client.GetAsync("/auth/providers");
        using var providers = JsonDocument.Parse(
            await providersResponse.Content.ReadAsStringAsync());
        using var sessionResponse = await client.GetAsync("/auth/session");
        using var session = JsonDocument.Parse(await sessionResponse.Content.ReadAsStringAsync());
        using var signInResponse = await client.GetAsync("/auth/sign-in/github?returnUrl=//evil.test");
        using var signIn = JsonDocument.Parse(await signInResponse.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(providersResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                providersResponse.Headers.CacheControl?.NoStore,
                Is.True);
            Assert.That(providers.RootElement.GetProperty("providers").GetArrayLength(), Is.Zero);
            Assert.That(sessionResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(sessionResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(session.RootElement.GetProperty("authenticated").GetBoolean(), Is.False);
            Assert.That(signInResponse.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                signIn.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("provider_not_configured"));
        });
    }

    [TestCase("//evil.test")]
    [TestCase("/\\evil.test")]
    [TestCase("https://evil.test/account")]
    public async Task ExternalSignInRejectsHostileReturnUrls(string returnUrl)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime,
            useTestAuthentication: false,
            useTestExternalProviders: true);
        using var client = factory.CreateApiClient();

        using var signIn = await client.GetAsync(
            $"/auth/sign-in/github?returnUrl={Uri.EscapeDataString(returnUrl)}");
        using var completed = await client.GetAsync(signIn.Headers.Location);

        Assert.Multiple(() =>
        {
            Assert.That(signIn.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completed.StatusCode, Is.EqualTo(HttpStatusCode.Redirect));
            Assert.That(completed.Headers.Location?.OriginalString, Is.EqualTo("/"));
        });
    }

    [Test]
    public async Task SignOutRejectsAMissingAntiforgeryToken()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "sign-out-user", "sign-out@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.PostAsync("/auth/sign-out", content: null);

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
    }

    [Test]
    public async Task SessionListsTheLinkedIdentityButRequiresRecentAuthenticationForUnlink()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "session-user", "session@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddMinutes(1));
        using var client = factory.CreateApiClient(signup.UserId);

        using var sessionResponse = await client.GetAsync("/auth/session");
        using var session = JsonDocument.Parse(await sessionResponse.Content.ReadAsStringAsync());
        var request = new HttpRequestMessage(HttpMethod.Delete, "/auth/providers/github");
        AntiforgeryTestClient.AddToken(request, await AntiforgeryTestClient.GetTokenAsync(client));
        using var unlinkResponse = await client.SendAsync(request);
        using var unlink = JsonDocument.Parse(await unlinkResponse.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(sessionResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(session.RootElement.GetProperty("authenticated").GetBoolean(), Is.True);
            Assert.That(session.RootElement.GetProperty("email").GetString(), Is.EqualTo("session@example.com"));
            Assert.That(session.RootElement.GetProperty("linkedProviders")[0].GetString(), Is.EqualTo("github"));
            Assert.That(session.RootElement.GetProperty("recentlyAuthenticated").GetBoolean(), Is.False);
            Assert.That(unlinkResponse.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(
                unlink.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("recent_authentication_required"));
        });
    }

    private static string? AuthenticationError(HttpResponseMessage response)
    {
        var location = response.Headers.Location
            ?? throw new AssertionException("The authentication response did not redirect.");
        var redirect = location.OriginalString;
        var queryIndex = redirect.IndexOf('?', StringComparison.Ordinal);
        var query = queryIndex < 0 ? string.Empty : redirect[queryIndex..];
        return QueryHelpers.ParseQuery(query).TryGetValue(
            AccountAuthentication.ErrorQueryParameter,
            out var error)
                ? error.SingleOrDefault()
                : null;
    }

    private static string RedirectPath(HttpResponseMessage response)
    {
        var location = response.Headers.Location
            ?? throw new AssertionException("The authentication response did not redirect.");
        var redirect = location.OriginalString;
        var queryIndex = redirect.IndexOf('?', StringComparison.Ordinal);
        return queryIndex < 0 ? redirect : redirect[..queryIndex];
    }

    private static string SessionCookie(HttpResponseMessage response)
    {
        return Cookie(response, "__Host-styrhous-session");
    }

    private static string Cookie(HttpResponseMessage response, string name)
    {
        return response.Headers.GetValues("Set-Cookie")
            .Last(value => value.StartsWith(
                $"{name}=",
                StringComparison.Ordinal))
            .Split(';', 2)[0];
    }
}
