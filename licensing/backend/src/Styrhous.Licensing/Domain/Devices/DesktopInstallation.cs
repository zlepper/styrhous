using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Devices;

public sealed record DesktopInstallation
{
    public const int MaximumDisplayNameLength = 120;
    public const int MaximumPlatformLength = 64;
    public const int MaximumArchitectureLength = 32;
    public const int MaximumVersionLength = 64;

    private DesktopInstallation(
        Guid installationId,
        string displayName,
        string platform,
        string architecture,
        string styrhousVersion)
    {
        InstallationId = installationId;
        DisplayName = displayName;
        Platform = platform;
        Architecture = architecture;
        StyrhousVersion = styrhousVersion;
    }

    public Guid InstallationId { get; }

    public string DisplayName { get; }

    public string Platform { get; }

    public string Architecture { get; }

    public string StyrhousVersion { get; }

    public static DesktopInstallation Create(
        Guid installationId,
        string displayName,
        string platform,
        string architecture,
        string styrhousVersion)
    {

        return new DesktopInstallation(
            installationId,
            RequiredText.Normalize(displayName, nameof(displayName), MaximumDisplayNameLength),
            RequiredText.Normalize(platform, nameof(platform), MaximumPlatformLength),
            RequiredText.Normalize(architecture, nameof(architecture), MaximumArchitectureLength),
            RequiredText.Normalize(styrhousVersion, nameof(styrhousVersion), MaximumVersionLength));
    }
}
