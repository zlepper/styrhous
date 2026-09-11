using System.Globalization;
using System.Security.Claims;
using Microsoft.AspNetCore.Authentication;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Antiforgery;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Api.Authentication;

public static class AccountAuthenticationEndpoints
{
    private const string AuthTimeClaim = "styrhous:auth_time";
    private static readonly TimeSpan RecentAuthenticationWindow = TimeSpan.FromMinutes(10);

    public static IEndpointRouteBuilder MapAccountAuthenticationEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapGet("/auth/providers", ProvidersAsync);
        endpoints.MapGet("/auth/session", SessionAsync);
        endpoints.MapGet("/auth/sign-in/{provider}", SignInAsync);
        endpoints.MapGet("/auth/link/{provider}", LinkAsync).RequireAuthorization();
        endpoints.MapGet("/auth/reauth/{provider}", ReauthenticateAsync)
            .RequireAuthorization();
        endpoints.MapGet("/auth/callback/{provider}", CompleteAsync);
        endpoints.MapDelete("/auth/providers/{provider}", UnlinkAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPost("/auth/sign-out", (Delegate)SignOutAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        return endpoints;
    }

    private static async Task<IResult> ProvidersAsync(
        IAuthenticationSchemeProvider schemes)
    {
        var providers = await ConfiguredProvidersAsync(schemes);
        return TypedResults.Ok(new { providers });
    }

    private static async Task<IResult> SessionAsync(
        ClaimsPrincipal principal,
        LicensingDbContext dbContext,
        ExternalAccountService accounts,
        IAuthenticationSchemeProvider schemes,
        TimeProvider timeProvider,
        CancellationToken cancellationToken)
    {
        var configuredProviders = await ConfiguredProvidersAsync(schemes);
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Ok(new
            {
                authenticated = false,
                configuredProviders,
            });
        }

        var email = await dbContext.UserAccounts.AsNoTracking()
            .Where(user => user.Id == userId)
            .Select(user => user.VerifiedEmail)
            .SingleOrDefaultAsync(cancellationToken);
        if (email is null)
        {
            return TypedResults.Unauthorized();
        }

        return TypedResults.Ok(new
        {
            authenticated = true,
            userId,
            email,
            linkedProviders = await accounts.ListProvidersAsync(userId, cancellationToken),
            configuredProviders,
            recentlyAuthenticated = IsRecent(principal, timeProvider.GetUtcNow()),
        });
    }

    private static async Task<IResult> SignInAsync(
        string provider,
        string? returnUrl,
        HttpContext httpContext,
        IAuthenticationSchemeProvider schemes)
    {
        if (!await IsConfiguredProviderAsync(provider, schemes))
        {
            return ProviderNotConfigured();
        }

        return await ChallengeAsync(
            provider,
            AccountAuthentication.SignInOperation,
            AccountAuthentication.SafeReturnUrl(returnUrl),
            userId: null,
            httpContext: httpContext);
    }

    private static async Task<IResult> LinkAsync(
        string provider,
        string? returnUrl,
        ClaimsPrincipal principal,
        HttpContext httpContext,
        IAuthenticationSchemeProvider schemes,
        TimeProvider timeProvider)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        if (!IsRecent(principal, timeProvider.GetUtcNow()))
        {
            return TypedResults.Json(
                new { reasonCode = "recent_authentication_required" },
                statusCode: StatusCodes.Status403Forbidden);
        }

        if (!await IsConfiguredProviderAsync(provider, schemes))
        {
            return ProviderNotConfigured();
        }

        return await ChallengeAsync(
            provider,
            AccountAuthentication.LinkOperation,
            AccountAuthentication.SafeReturnUrl(returnUrl),
            userId,
            httpContext);
    }

    private static async Task<IResult> ReauthenticateAsync(
        string provider,
        string? returnUrl,
        ClaimsPrincipal principal,
        HttpContext httpContext,
        IAuthenticationSchemeProvider schemes)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        if (!await IsConfiguredProviderAsync(provider, schemes))
        {
            return ProviderNotConfigured();
        }

        return await ChallengeAsync(
            provider,
            AccountAuthentication.ReauthenticationOperation,
            AccountAuthentication.SafeReturnUrl(returnUrl),
            userId,
            httpContext);
    }

    private static async Task<IResult> ChallengeAsync(
        string provider,
        string operation,
        string returnUrl,
        Guid? userId,
        HttpContext httpContext)
    {
        await httpContext.SignOutAsync(AccountAuthentication.ExternalScheme);
        var callback = $"/auth/callback/{Uri.EscapeDataString(provider)}";
        var properties = new AuthenticationProperties
        {
            RedirectUri = callback,
        };
        properties.Items[AccountAuthentication.OperationProperty] = operation;
        properties.Items[AccountAuthentication.ProviderProperty] = provider;
        properties.Items[AccountAuthentication.ReturnUrlProperty] = returnUrl;
        if (userId is not null)
        {
            properties.Items[AccountAuthentication.LinkUserProperty] = userId.Value.ToString();
        }

        return Results.Challenge(properties, [provider]);
    }

    private static async Task<IResult> CompleteAsync(
        string provider,
        HttpContext httpContext,
        UserSignupService signups,
        ExternalAccountService accounts,
        TimeProvider timeProvider,
        CancellationToken cancellationToken)
    {
        var external = await httpContext.AuthenticateAsync(
            AccountAuthentication.ExternalScheme);
        var returnUrl = AccountAuthentication.SafeReturnUrl(GetItem(
            external.Properties,
            AccountAuthentication.ReturnUrlProperty));
        var subject = external.Principal?.FindFirstValue(ClaimTypes.NameIdentifier);
        var verifiedEmail = external.Principal?.FindFirstValue(
            AccountAuthentication.VerifiedEmailClaim);
        if (!external.Succeeded
            || string.IsNullOrWhiteSpace(subject)
            || string.IsNullOrWhiteSpace(verifiedEmail))
        {
            await httpContext.SignOutAsync(AccountAuthentication.ExternalScheme);
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                provider.Equals(AccountAuthenticationProviders.Microsoft, StringComparison.OrdinalIgnoreCase)
                    ? "microsoft_verified_email_required"
                    : "verified_email_required");
        }

        var normalizedProvider = provider.Trim().ToLowerInvariant();
        if (!string.Equals(
                GetItem(external.Properties, AccountAuthentication.ProviderProperty),
                normalizedProvider,
                StringComparison.Ordinal))
        {
            await httpContext.SignOutAsync(AccountAuthentication.ExternalScheme);
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                "authentication_provider_changed");
        }

        VerifiedExternalIdentity identity;
        try
        {
            identity = VerifiedExternalIdentity.Create(
                normalizedProvider,
                subject,
                verifiedEmail);
        }
        catch (ArgumentException)
        {
            await httpContext.SignOutAsync(AccountAuthentication.ExternalScheme);
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                "invalid_external_identity");
        }

        var operation = GetItem(
            external.Properties,
            AccountAuthentication.OperationProperty);
        if (operation is not AccountAuthentication.SignInOperation
            and not AccountAuthentication.LinkOperation
            and not AccountAuthentication.ReauthenticationOperation)
        {
            await httpContext.SignOutAsync(AccountAuthentication.ExternalScheme);
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                "authentication_operation_invalid");
        }

        Guid authenticatedUserId;
        try
        {
            if (operation is AccountAuthentication.LinkOperation
                or AccountAuthentication.ReauthenticationOperation)
            {
                if (!Guid.TryParse(
                        GetItem(
                            external.Properties,
                            AccountAuthentication.LinkUserProperty),
                        out var userId)
                    || !AuthenticatedUser.TryGetId(httpContext.User, out var sessionUserId)
                    || userId != sessionUserId)
                {
                    return AccountAuthentication.FailureRedirect(
                        returnUrl,
                        "authentication_session_changed");
                }

                if (operation == AccountAuthentication.LinkOperation)
                {
                    await accounts.LinkAsync(userId, identity, cancellationToken);
                    authenticatedUserId = userId;
                }
                else
                {
                    if (!await accounts.IsLinkedAsync(userId, identity, cancellationToken))
                    {
                        return AccountAuthentication.FailureRedirect(
                            returnUrl,
                            "provider_not_linked");
                    }

                    authenticatedUserId = userId;
                }
            }
            else
            {
                authenticatedUserId = (await signups.SignUpAsync(identity, cancellationToken)).UserId;
            }
        }
        catch (AccountLinkRequiredException)
        {
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                "account_link_required");
        }
        catch (AccountLinkEmailMismatchException)
        {
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                "verified_email_mismatch");
        }
        catch (AccountProviderAlreadyLinkedException)
        {
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                "provider_already_linked");
        }
        catch (Exception exception) when (EfConcurrencyFailure.IsRetryable(exception))
        {
            return AccountAuthentication.FailureRedirect(
                returnUrl,
                "concurrent_modification");
        }
        finally
        {
            await httpContext.SignOutAsync(AccountAuthentication.ExternalScheme);
        }

        var now = timeProvider.GetUtcNow();
        var claims = new[]
        {
            new Claim(ClaimTypes.NameIdentifier, authenticatedUserId.ToString()),
            new Claim(ClaimTypes.Email, verifiedEmail),
            new Claim(AuthTimeClaim, now.ToUnixTimeSeconds().ToString(CultureInfo.InvariantCulture)),
            new Claim(ClaimTypes.AuthenticationMethod, normalizedProvider),
        };
        await httpContext.SignInAsync(
            AccountAuthentication.SessionScheme,
            new ClaimsPrincipal(new ClaimsIdentity(
                claims,
                AccountAuthentication.SessionScheme)),
            new AuthenticationProperties
            {
                IsPersistent = true,
                IssuedUtc = now,
                ExpiresUtc = now.AddDays(30),
            });
        return Results.Redirect(returnUrl);
    }

    private static async Task<IResult> UnlinkAsync(
        string provider,
        ClaimsPrincipal principal,
        ExternalAccountService accounts,
        TimeProvider timeProvider,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        if (!IsRecent(principal, timeProvider.GetUtcNow()))
        {
            return TypedResults.Json(
                new { reasonCode = "recent_authentication_required" },
                statusCode: StatusCodes.Status403Forbidden);
        }

        try
        {
            await accounts.UnlinkAsync(
                userId,
                provider.Trim().ToLowerInvariant(),
                cancellationToken);
            return TypedResults.Ok(new { reasonCode = "provider_unlinked" });
        }
        catch (AccountProviderNotLinkedException)
        {
            return TypedResults.NotFound(new { reasonCode = "provider_not_linked" });
        }
        catch (LastAccountProviderException)
        {
            return TypedResults.Conflict(new { reasonCode = "last_provider_required" });
        }
        catch (Exception exception) when (EfConcurrencyFailure.IsRetryable(exception))
        {
            return TypedResults.Conflict(new { reasonCode = "concurrent_modification" });
        }
    }

    private static async Task<IResult> SignOutAsync(
        HttpContext httpContext)
    {
        await httpContext.SignOutAsync(AccountAuthentication.SessionScheme);
        return TypedResults.NoContent();
    }

    private static bool IsRecent(ClaimsPrincipal principal, DateTimeOffset now)
    {
        if (!long.TryParse(
            principal.FindFirstValue(AuthTimeClaim),
            NumberStyles.None,
            CultureInfo.InvariantCulture,
            out var value))
        {
            return false;
        }

        var elapsed = now - DateTimeOffset.FromUnixTimeSeconds(value);
        return elapsed >= TimeSpan.Zero && elapsed <= RecentAuthenticationWindow;
    }

    private static async Task<string[]> ConfiguredProvidersAsync(
        IAuthenticationSchemeProvider schemes)
    {
        var configured = new List<string>();
        foreach (var provider in AccountAuthenticationProviders.All)
        {
            if (await IsConfiguredProviderAsync(provider.Scheme, schemes))
            {
                configured.Add(provider.Scheme);
            }
        }

        return configured.ToArray();
    }

    private static async Task<bool> IsConfiguredProviderAsync(
        string provider,
        IAuthenticationSchemeProvider schemes)
    {
        return AccountAuthenticationProviders.IsSupported(provider)
        && await schemes.GetSchemeAsync(provider) is not null;
    }

    private static string? GetItem(AuthenticationProperties? properties, string key)
    {
        return properties is not null && properties.Items.TryGetValue(key, out var value)
            ? value
            : null;
    }

    private static Microsoft.AspNetCore.Http.HttpResults.NotFound<
        AccountAuthenticationErrorResponse> ProviderNotConfigured()
    {
        return TypedResults.NotFound(new AccountAuthenticationErrorResponse(
            "provider_not_configured"));
    }
}

public sealed record AccountAuthenticationErrorResponse(string ReasonCode);
