using Microsoft.AspNetCore;
using OpenIddict.Abstractions;
using OpenIddict.Server;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Persistence;
using static OpenIddict.Abstractions.OpenIddictConstants;
using static OpenIddict.Server.OpenIddictServerEvents;

namespace Styrhous.Licensing.Api.Desktop;

internal sealed class DesktopRevocationSerializationHandler(
    LicensingDbContext dbContext)
    : IOpenIddictServerHandler<HandleRevocationRequestContext>
{

    public async ValueTask HandleAsync(HandleRevocationRequestContext context)
    {
        if (Guid.TryParse(
                context.GenericTokenPrincipal?.GetClaim(Claims.Subject),
                out var userId)
            && Guid.TryParse(
                context.GenericTokenPrincipal?.GetAuthorizationId(),
                out var authorizationId))
        {
            var request = context.Transaction.GetHttpRequest()
                ?? throw new InvalidOperationException(
                    "The desktop revocation request has no HTTP request.");
            DesktopProtocolRequestState.SetRevocation(
                request.HttpContext,
                userId,
                authorizationId);
            await EfTransactionSerialization.TryClaimUserAsync(
                dbContext,
                userId,
                request.HttpContext.RequestAborted);
        }
    }
}
