using System.Security.Claims;
using System.Security.Cryptography.X509Certificates;
using Microsoft.AspNetCore;
using OpenIddict.Abstractions;
using OpenIddict.EntityFrameworkCore;
using OpenIddict.Server;
using Styrhous.Licensing.Persistence;
using static OpenIddict.Abstractions.OpenIddictConstants;
using static OpenIddict.Server.OpenIddictServerEvents;

namespace Styrhous.Licensing.Api.Desktop;

public static class DesktopProtocolConfiguration
{
    public static void AddDesktopProtocol(
        this IServiceCollection services,
        Uri issuer,
        X509Certificate2 currentCertificate,
        IReadOnlyCollection<X509Certificate2> previousCertificates)
    {
        ArgumentNullException.ThrowIfNull(services);
        ArgumentNullException.ThrowIfNull(issuer);
        ArgumentNullException.ThrowIfNull(currentCertificate);
        ArgumentNullException.ThrowIfNull(previousCertificates);

        services.AddOpenIddict()
            .AddCore(options =>
            {
                options.UseEntityFrameworkCore()
                    .UseDbContext<LicensingDbContext>()
                    .ReplaceDefaultEntities<Guid>();
            })
            .AddServer(options =>
            {
                options.SetIssuer(issuer)
                    .SetDeviceAuthorizationEndpointUris(
                        DesktopProtocolConstants.DeviceAuthorizationPath)
                    .SetEndUserVerificationEndpointUris(
                        DesktopProtocolConstants.ApprovalPath)
                    .SetTokenEndpointUris(DesktopProtocolConstants.TokenPath)
                    .SetRevocationEndpointUris(DesktopProtocolConstants.RevocationPath)
                    .AllowDeviceAuthorizationFlow()
                    .AllowRefreshTokenFlow()
                    .IgnoreEndpointPermissions()
                    .IgnoreGrantTypePermissions()
                    .IgnoreScopePermissions()
                    .RegisterScopes(DesktopProtocolConstants.Scope)
                    .SetAccessTokenLifetime(DesktopProtocolConstants.AccessTokenLifetime)
                    .SetDeviceCodeLifetime(DesktopProtocolConstants.DeviceCodeLifetime)
                    .SetUserCodeLifetime(DesktopProtocolConstants.DeviceCodeLifetime)
                    .SetRefreshTokenLifetime(DesktopProtocolConstants.RefreshTokenLifetime)
                    .SetRefreshTokenReuseLeeway(leeway: null)
                    .DisableSlidingRefreshTokenExpiration()
                    .UseReferenceRefreshTokens()
                    .AddEncryptionCertificate(currentCertificate)
                    .AddSigningCertificate(currentCertificate);
                foreach (var certificate in previousCertificates)
                {
                    options.AddEncryptionCertificate(certificate)
                        .AddSigningCertificate(certificate);
                }

                options.AddEventHandler<ValidateDeviceAuthorizationRequestContext>(builder =>
                {
                    builder.UseInlineHandler(ValidateDeviceAuthorizationRequestAsync);
                });
                options.AddEventHandler<HandleDeviceAuthorizationRequestContext>(builder =>
                {
                    builder.UseInlineHandler(HandleDeviceAuthorizationRequestAsync);
                });
                options.AddEventHandler<ApplyDeviceAuthorizationResponseContext>(builder =>
                {
                    builder.UseInlineHandler(
                        context => ApplyDeviceAuthorizationResponseAsync(context, issuer));
                });
                options.AddEventHandler<ApplyEndUserVerificationResponseContext>(builder =>
                {
                    builder.UseInlineHandler(ApplyEndUserVerificationResponseAsync);
                });
                options.AddEventHandler<HandleRevocationRequestContext>(builder =>
                {
                    builder.SetOrder(
                            OpenIddictServerHandlers.Revocation.RevokeToken.Descriptor.Order + 500)
                        .UseScopedHandler<DesktopRevocationRequestHandler>();
                });
                options.AddEventHandler<HandleRevocationRequestContext>(builder =>
                {
                    builder.SetOrder(
                            OpenIddictServerHandlers.Revocation.RevokeToken.Descriptor.Order - 500)
                        .UseScopedHandler<DesktopRevocationSerializationHandler>();
                });

                options.UseAspNetCore()
                    .EnableEndUserVerificationEndpointPassthrough()
                    .EnableTokenEndpointPassthrough();
            })
            .AddValidation(options =>
            {
                options.AddAudiences(DesktopProtocolConstants.Resource);
                options.UseLocalServer();
                options.UseAspNetCore();
            });
    }

    private static ValueTask ValidateDeviceAuthorizationRequestAsync(
        ValidateDeviceAuthorizationRequestContext context)
    {
        var scopes = context.Request.GetScopes().ToHashSet(StringComparer.Ordinal);
        if (!scopes.SetEquals([DesktopProtocolConstants.Scope, Scopes.OfflineAccess]))
        {
            context.Reject(
                Errors.InvalidScope,
                "The desktop client must request the supported desktop and offline scopes.");
            return ValueTask.CompletedTask;
        }

        if (!DesktopInstallationProtocolClaims.TryCreate(
                context.Request,
                out _))
        {
            context.Reject(
                Errors.InvalidRequest,
                "The desktop installation metadata is invalid.");
        }

        return ValueTask.CompletedTask;
    }

    private static ValueTask HandleDeviceAuthorizationRequestAsync(
        HandleDeviceAuthorizationRequestContext context)
    {
        if (!DesktopInstallationProtocolClaims.TryCreate(
                context.Request,
                out var installation))
        {
            context.Reject(
                Errors.InvalidRequest,
                "The desktop installation metadata is invalid.");
            return ValueTask.CompletedTask;
        }

        context.Principal = new ClaimsPrincipal(new ClaimsIdentity())
            .SetScopes(context.Request.GetScopes())
            .SetInstallation(installation);
        return ValueTask.CompletedTask;
    }

    private static ValueTask ApplyDeviceAuthorizationResponseAsync(
        ApplyDeviceAuthorizationResponseContext context,
        Uri issuer)
    {
        if (context.Error is not null)
        {
            return ValueTask.CompletedTask;
        }

        var verificationUri = new Uri(issuer, DesktopProtocolConstants.VerificationPath);
        context.Response.VerificationUri = verificationUri.AbsoluteUri;
        context.Response.VerificationUriComplete = string.IsNullOrEmpty(context.Response.UserCode)
            ? null
            : $"{verificationUri.AbsoluteUri}?user_code="
                + Uri.EscapeDataString(context.Response.UserCode);
        context.Response.ExpiresIn = (long)
            DesktopProtocolConstants.DeviceCodeLifetime.TotalSeconds;
        context.Response[Parameters.Interval] = DesktopProtocolConstants.PollingIntervalSeconds;
        return ValueTask.CompletedTask;
    }

    private static ValueTask ApplyEndUserVerificationResponseAsync(
        ApplyEndUserVerificationResponseContext context)
    {
        var decision = (string?)context.Request?["decision"];
        var successfulDecision = decision switch
        {
            "approve" => context.Error is null,
            "deny" => context.Error is Errors.AccessDenied,
            _ => false,
        };
        if (!successfulDecision)
        {
            return ValueTask.CompletedTask;
        }

        var request = context.Transaction.GetHttpRequest()
            ?? throw new InvalidOperationException(
                "The desktop verification response has no HTTP request.");
        var response = request.HttpContext.Response;
        response.StatusCode = StatusCodes.Status204NoContent;
        context.HandleRequest();
        return ValueTask.CompletedTask;
    }

}
