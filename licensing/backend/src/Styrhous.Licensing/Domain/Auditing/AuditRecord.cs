
namespace Styrhous.Licensing.Domain.Auditing;

public enum AuditAction
{
    BillingCheckoutStarted,
    BillingSeatQuantityChangeStarted,
    DeviceActivated,
    DeviceActivationRemoved,
    DeviceRevokedManual,
    DeviceRevokedStale,
    OrganizationCreated,
    OrganizationInvitationAccepted,
    OrganizationInvitationCancelled,
    OrganizationInvitationCreated,
    OrganizationInvitationResent,
    OrganizationMemberAssigned,
    OrganizationMemberRemoved,
    OrganizationMemberRoleChanged,
    OrganizationOwnerAssigned,
    SeatAssigned,
    SeatUnassigned,
    TrialTransferred,
}

public enum AuditTargetType
{
    BillingOperation,
    DeviceActivation,
    Organization,
    OrganizationInvitation,
    OrganizationMembership,
    Seat,
    Trial,
}

public sealed class AuditRecord
{
    private AuditRecord()
    {
    }

    private AuditRecord(
        Guid id,
        Guid correlationId,
        Guid actorUserId,
        AuditAction action,
        AuditTargetType targetType,
        Guid targetId,
        DateTimeOffset occurredAt)
    {
        Id = id;
        CorrelationId = correlationId;
        ActorUserId = actorUserId;
        Action = action;
        TargetType = targetType;
        TargetId = targetId;
        OccurredAt = occurredAt;
    }

    public Guid Id { get; private set; }

    public Guid CorrelationId { get; private set; }

    public Guid ActorUserId { get; private set; }

    public AuditAction Action { get; private set; }

    public AuditTargetType TargetType { get; private set; }

    public Guid TargetId { get; private set; }

    public DateTimeOffset OccurredAt { get; private set; }

    internal static AuditRecord Create(
        Guid correlationId,
        Guid actorUserId,
        AuditAction action,
        AuditTargetType targetType,
        Guid targetId,
        DateTimeOffset occurredAt)
    {

        return new AuditRecord(
            Guid.CreateVersion7(),
            correlationId,
            actorUserId,
            action,
            targetType,
            targetId,
            occurredAt.ToUniversalTime());
    }
}
