namespace Styrhous.Licensing.Api.Desktop;

public static class DesktopProtocolConstants
{
    public const string ClientId = "styrhous-desktop";

    public const string Scope = "styrhous.desktop";

    public const string Resource = "styrhous-desktop-api";

    public const string DeviceAuthorizationPath = "/desktop/v1/device/authorize";

    public const string ApprovalPath = "/desktop/v1/device/approval";

    public const string VerificationPath = "/desktop/v1/device/verify";

    public const string TokenPath = "/desktop/v1/token";

    public const string RevocationPath = "/desktop/v1/token/revoke";

    public const string EntitlementPath = "/desktop/v1/entitlement";

    public const string KeysPath = "/desktop/v1/keys";

    public const string DevicesPath = "/desktop/v1/devices";

    public const string LeaseAudience = "styrhous-desktop";

    public const int PollingIntervalSeconds = 5;

    public static readonly TimeSpan AccessTokenLifetime = TimeSpan.FromMinutes(15);

    public static readonly TimeSpan DeviceCodeLifetime = TimeSpan.FromMinutes(10);

    public static readonly TimeSpan RefreshTokenLifetime = TimeSpan.FromDays(90);

    public static readonly TimeSpan LeaseLifetime = TimeSpan.FromDays(7);

    public static readonly TimeSpan LeaseRefreshInterval = TimeSpan.FromHours(24);

    public static class Claims
    {
        public const string InstallationId = "styrhous:installation_id";

        public const string DisplayName = "styrhous:display_name";

        public const string Platform = "styrhous:platform";

        public const string Architecture = "styrhous:architecture";

        public const string Version = "styrhous:version";

        public const string SeatId = "styrhous:seat_id";

        public const string BillingAccountId = "styrhous:billing_account_id";

        public const string ActivationId = "styrhous:activation_id";

        public const string EntitlementState = "styrhous:entitlement_state";

        public const string EntitlementReasonCode = "styrhous:entitlement_reason_code";
    }
}
