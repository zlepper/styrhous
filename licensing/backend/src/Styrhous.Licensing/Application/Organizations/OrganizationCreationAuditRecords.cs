using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Domain.Trials;

namespace Styrhous.Licensing.Application.Organizations;

internal static class OrganizationCreationAuditRecords
{
    public static AuditRecord[] Create(
        OrganizationRegistration registration,
        Guid correlationId,
        Trial? transferredTrial)
    {
        var actorUserId = registration.OwnerMembership.UserId;
        var records = new List<AuditRecord>(capacity: transferredTrial is null ? 3 : 4)
        {
            AuditRecord.Create(
                correlationId,
                actorUserId,
                AuditAction.OrganizationCreated,
                AuditTargetType.Organization,
                registration.Organization.Id,
                registration.ObservedAt),
            AuditRecord.Create(
                correlationId,
                actorUserId,
                AuditAction.OrganizationOwnerAssigned,
                AuditTargetType.OrganizationMembership,
                registration.OwnerMembership.Id,
                registration.ObservedAt),
            AuditRecord.Create(
                correlationId,
                actorUserId,
                AuditAction.SeatAssigned,
                AuditTargetType.Seat,
                registration.Seat.Id,
                registration.ObservedAt),
        };
        if (transferredTrial is not null)
        {
            records.Add(
                AuditRecord.Create(
                    correlationId,
                    actorUserId,
                    AuditAction.TrialTransferred,
                    AuditTargetType.Trial,
                    transferredTrial.Id,
                    registration.ObservedAt));
        }

        return records.ToArray();
    }
}
