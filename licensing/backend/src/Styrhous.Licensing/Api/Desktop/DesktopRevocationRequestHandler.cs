using Microsoft.AspNetCore;
using OpenIddict.Abstractions;
using OpenIddict.Server;
using Styrhous.Licensing.Persistence;
using static OpenIddict.Server.OpenIddictServerEvents;

namespace Styrhous.Licensing.Api.Desktop;

internal sealed class DesktopRevocationRequestHandler(
    LicensingDbContext dbContext,
    DesktopProtocolTransaction transaction,
    TimeProvider timeProvider)
    : IOpenIddictServerHandler<HandleRevocationRequestContext>
{

    public async ValueTask HandleAsync(HandleRevocationRequestContext context)
    {
        if (!Guid.TryParse(
                context.GenericTokenPrincipal?.GetAuthorizationId(),
                out var authorizationId))
        {
            return;
        }

        var request = context.Transaction.GetHttpRequest()
            ?? throw new InvalidOperationException(
                "The desktop revocation request has no HTTP request.");
        await transaction.EnsureStartedAsync(request.HttpContext.RequestAborted);
        await DesktopSessionRevocation.RevokeDesktopAuthorizationsAsync(
            dbContext,
            [authorizationId],
            timeProvider.GetUtcNow(),
            request.HttpContext.RequestAborted);
        await dbContext.SaveChangesAsync(request.HttpContext.RequestAborted);
    }
}
