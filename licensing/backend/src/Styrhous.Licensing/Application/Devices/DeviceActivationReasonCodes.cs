namespace Styrhous.Licensing.Application.Devices;

public static class DeviceActivationReasonCodes
{
    public const string Activated = "device_activated";
    public const string AlreadyActive = "device_already_active";
    public const string StaleDeviceReplaced = "stale_device_replaced";
    public const string DeviceLimitReached = "device_limit_reached";
    public const string SeatNotEligible = "seat_not_eligible";
}
