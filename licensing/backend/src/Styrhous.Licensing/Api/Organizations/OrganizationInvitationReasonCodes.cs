namespace Styrhous.Licensing.Api.Organizations;

public static class OrganizationInvitationReasonCodes
{
    public const string Listed = "organization_invitations_listed";

    public const string Created = "invitation_created";

    public const string Accepted = "invitation_accepted";

    public const string Cancelled = "invitation_cancelled";

    public const string CancellationSuperseded = "invitation_cancellation_superseded";

    public const string Resent = "invitation_resent";

    public const string ResendSuperseded = "invitation_resend_superseded";

    public const string OrganizationNotFound = OrganizationReasonCodes.OrganizationNotFound;

    public const string InsufficientPermission = "insufficient_permission";

    public const string AlreadyMember = "already_member";

    public const string InvitationAlreadyPending = "invitation_already_pending";

    public const string InvitationNotFound = "invitation_not_found";

    public const string EmailMismatch = "invitation_email_mismatch";

    public const string NoActiveSeatCapacity = "no_active_seat_capacity";

    public const string SeatCapacityReached = "seat_capacity_reached";
}
