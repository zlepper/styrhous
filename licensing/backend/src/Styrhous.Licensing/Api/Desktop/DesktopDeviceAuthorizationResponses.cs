using Styrhous.Licensing.Application.Desktop;

namespace Styrhous.Licensing.Api.Desktop;

public sealed record DesktopDeviceAuthorizationApprovalResponse(
    string ReasonCode,
    DesktopDeviceAuthorizationInstallationResponse Installation,
    IReadOnlyList<DesktopDeviceAuthorizationSeatResponse> EligibleSeats,
    Guid? SelectedSeatId)
{
    public static DesktopDeviceAuthorizationApprovalResponse From(
        DesktopDeviceAuthorizationApproval approval)
    {
        return new(
            approval.ReasonCode,
            DesktopDeviceAuthorizationInstallationResponse.From(approval.Installation),
            approval.EligibleSeats.Select(
                DesktopDeviceAuthorizationSeatResponse.From).ToArray(),
            approval.SelectedSeatId);
    }
}

public sealed record DesktopDeviceAuthorizationInstallationResponse(
    Guid InstallationId,
    string DisplayName,
    string Platform,
    string Architecture,
    string StyrhousVersion)
{
    internal static DesktopDeviceAuthorizationInstallationResponse From(
        Domain.Devices.DesktopInstallation installation)
    {
        return new(
            installation.InstallationId,
            installation.DisplayName,
            installation.Platform,
            installation.Architecture,
            installation.StyrhousVersion);
    }
}

public sealed record DesktopDeviceAuthorizationSeatResponse(
    Guid SeatId,
    Guid BillingAccountId,
    string Name,
    string EntitlementState,
    string EntitlementReasonCode,
    int DeviceLimit,
    bool CanActivate,
    IReadOnlyList<DesktopDeviceAuthorizationActiveDeviceResponse> ActiveDevices)
{
    public static DesktopDeviceAuthorizationSeatResponse From(
        DesktopDeviceAuthorizationSeat seat)
    {
        return new(
            seat.SeatId,
            seat.BillingAccountId,
            seat.Name,
            seat.EntitlementState.ToString().ToLowerInvariant(),
            seat.EntitlementReasonCode,
            seat.DeviceLimit,
            seat.CanActivate,
            seat.ActiveDevices.Select(
                DesktopDeviceAuthorizationActiveDeviceResponse.From).ToArray());
    }
}

public sealed record DesktopDeviceAuthorizationActiveDeviceResponse(
    Guid ActivationId,
    Guid InstallationId,
    string DisplayName,
    string Platform,
    string Architecture,
    string StyrhousVersion,
    DateTimeOffset ActivatedAt,
    DateTimeOffset LastSeenAt)
{
    internal static DesktopDeviceAuthorizationActiveDeviceResponse From(
        Application.Devices.ActiveDevice device)
    {
        return new(
            device.ActivationId,
            device.InstallationId,
            device.DisplayName,
            device.Platform,
            device.Architecture,
            device.StyrhousVersion,
            device.ActivatedAt,
            device.LastSeenAt);
    }
}

public sealed record DesktopDeviceAuthorizationErrorResponse(string ReasonCode)
{
    public static DesktopDeviceAuthorizationErrorResponse NotFound()
    {
        return new("device_authorization_not_found");
    }

    public static DesktopDeviceAuthorizationErrorResponse InvalidRequest()
    {
        return new(DesktopDeviceAuthorizationReasonCodes.InvalidRequest);
    }

    public static DesktopDeviceAuthorizationErrorResponse SeatRequired()
    {
        return new("device_authorization_seat_required");
    }

    public static DesktopDeviceAuthorizationErrorResponse SeatNotEligible()
    {
        return new(DesktopDeviceAuthorizationReasonCodes.SeatNotEligible);
    }

    public static DesktopDeviceAuthorizationErrorResponse ConcurrentModification()
    {
        return new(DesktopDeviceAuthorizationReasonCodes.ConcurrentModification);
    }
}

public sealed record DesktopDeviceAuthorizationCapacityResponse(
    string ReasonCode,
    IReadOnlyList<DesktopDeviceAuthorizationActiveDeviceResponse> ActiveDevices)
{
    public static DesktopDeviceAuthorizationCapacityResponse From(
        DesktopDeviceAuthorizationDecisionResult result)
    {
        return new(
            result.ReasonCode,
            result.ActiveDevices.Select(
                DesktopDeviceAuthorizationActiveDeviceResponse.From).ToArray());
    }
}
