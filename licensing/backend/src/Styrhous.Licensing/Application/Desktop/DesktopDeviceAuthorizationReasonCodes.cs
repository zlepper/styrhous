namespace Styrhous.Licensing.Application.Desktop;

public static class DesktopDeviceAuthorizationReasonCodes
{
    public const string InvalidRequest = "device_authorization_invalid_request";

    public const string AwaitingApproval = "device_authorization_awaiting_approval";

    public const string Approved = "device_authorization_approved";

    public const string SeatNotEligible = "device_authorization_seat_not_eligible";

    public const string DeviceLimitReached = "device_limit_reached";

    public const string ConcurrentModification =
        "device_authorization_concurrent_modification";
}
