using System.Security.Claims;
using OpenIddict.Abstractions;
using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Api.Desktop;

internal static class DesktopInstallationProtocolClaims
{
    private const string InstallationIdParameter = "installation_id";
    private const string DisplayNameParameter = "display_name";
    private const string PlatformParameter = "platform";
    private const string ArchitectureParameter = "architecture";
    private const string VersionParameter = "styrhous_version";

    public static bool TryCreate(
        OpenIddictRequest request,
        out DesktopInstallation installation)
    {
        ArgumentNullException.ThrowIfNull(request);
        var installationId = (string?)request[InstallationIdParameter];
        var displayName = (string?)request[DisplayNameParameter];
        var platform = (string?)request[PlatformParameter];
        var architecture = (string?)request[ArchitectureParameter];
        var version = (string?)request[VersionParameter];
        if (!Guid.TryParse(installationId, out var parsedInstallationId))
        {
            installation = null!;
            return false;
        }

        try
        {
            installation = DesktopInstallation.Create(
                parsedInstallationId,
                displayName ?? string.Empty,
                platform ?? string.Empty,
                architecture ?? string.Empty,
                version ?? string.Empty);
            return true;
        }
        catch (ArgumentException)
        {
            installation = null!;
            return false;
        }
    }

    public static DesktopInstallation? Read(ClaimsPrincipal principal)
    {
        ArgumentNullException.ThrowIfNull(principal);
        if (!Guid.TryParse(
                principal.GetClaim(DesktopProtocolConstants.Claims.InstallationId),
                out var installationId))
        {
            return null;
        }

        try
        {
            return DesktopInstallation.Create(
                installationId,
                principal.GetClaim(DesktopProtocolConstants.Claims.DisplayName)
                    ?? string.Empty,
                principal.GetClaim(DesktopProtocolConstants.Claims.Platform)
                    ?? string.Empty,
                principal.GetClaim(DesktopProtocolConstants.Claims.Architecture)
                    ?? string.Empty,
                principal.GetClaim(DesktopProtocolConstants.Claims.Version)
                    ?? string.Empty);
        }
        catch (ArgumentException)
        {
            return null;
        }
    }

    public static ClaimsPrincipal SetInstallation(
        this ClaimsPrincipal principal,
        DesktopInstallation installation)
    {
        ArgumentNullException.ThrowIfNull(principal);
        ArgumentNullException.ThrowIfNull(installation);
        return principal
            .SetClaim(
                DesktopProtocolConstants.Claims.InstallationId,
                installation.InstallationId.ToString())
            .SetClaim(
                DesktopProtocolConstants.Claims.DisplayName,
                installation.DisplayName)
            .SetClaim(DesktopProtocolConstants.Claims.Platform, installation.Platform)
            .SetClaim(
                DesktopProtocolConstants.Claims.Architecture,
                installation.Architecture)
            .SetClaim(DesktopProtocolConstants.Claims.Version, installation.StyrhousVersion);
    }
}
