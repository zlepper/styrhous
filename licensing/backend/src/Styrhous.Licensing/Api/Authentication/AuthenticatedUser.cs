using System.Security.Claims;

namespace Styrhous.Licensing.Api.Authentication;

internal static class AuthenticatedUser
{
    public static bool TryGetId(ClaimsPrincipal principal, out Guid userId)
    {
        var value = principal.FindFirstValue(ClaimTypes.NameIdentifier);
        return Guid.TryParse(value, out userId);
    }
}
