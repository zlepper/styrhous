using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

internal static class OrganizationInvitationAcceptanceAuditRecords
{
    public static AuditRecord[] Create(
        OrganizationInvitationAcceptance acceptance,
        Guid correlationId)
    {
        var actorUserId = acceptance.Membership.UserId;
        var records = new List<AuditRecord>
        {
            AuditRecord.Create(
                correlationId,
                actorUserId,
                AuditAction.OrganizationInvitationAccepted,
                AuditTargetType.OrganizationInvitation,
                acceptance.Invitation.Id,
                acceptance.ObservedAt),
            AuditRecord.Create(
                correlationId,
                actorUserId,
                AuditAction.OrganizationMemberAssigned,
                AuditTargetType.OrganizationMembership,
                acceptance.Membership.Id,
                acceptance.ObservedAt),
        };
        if (acceptance.Seat.ProductAccessEnabled)
        {
            records.Add(
                AuditRecord.Create(
                    correlationId,
                    actorUserId,
                    AuditAction.SeatAssigned,
                    AuditTargetType.Seat,
                    acceptance.Seat.Id,
                    acceptance.ObservedAt));
        }

        return records.ToArray();
    }
}
