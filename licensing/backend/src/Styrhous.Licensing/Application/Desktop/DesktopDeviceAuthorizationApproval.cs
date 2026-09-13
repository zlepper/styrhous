using Styrhous.Licensing.Domain.Devices;

namespace Styrhous.Licensing.Application.Desktop;

public sealed record DesktopDeviceAuthorizationApproval(
    string ReasonCode,
    DesktopInstallation Installation,
    IReadOnlyList<DesktopDeviceAuthorizationSeat> EligibleSeats,
    Guid? SelectedSeatId);
