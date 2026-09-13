using System.Security.Claims;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Api.Authentication;

internal static class AuthenticatedUser
{
    public static bool TryGetId(ClaimsPrincipal principal, out Guid userId)
    {
        var value = principal.FindFirstValue(ClaimTypes.NameIdentifier);
        return Guid.TryParse(value, out userId);
    }
}
