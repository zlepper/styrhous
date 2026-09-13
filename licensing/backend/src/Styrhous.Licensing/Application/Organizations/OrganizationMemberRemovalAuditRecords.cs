using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

internal static class OrganizationMemberRemovalAuditRecords
{
    public static AuditRecord[] Create(
        Guid actorUserId,
        OrganizationMembership membership,
        Seat seat,
        IReadOnlyCollection<DeviceActivation> deviceActivations,
        Guid correlationId,
        DateTimeOffset observedAt)
    {
        var records = new List<AuditRecord>(
            deviceActivations.Count + (seat.ProductAccessEnabled ? 2 : 1));
        records.AddRange(
            deviceActivations.Select(
                activation => AuditRecord.Create(
                    correlationId,
                    actorUserId,
                    AuditAction.DeviceActivationRemoved,
                    AuditTargetType.DeviceActivation,
                    activation.Id,
                    observedAt)));
        if (seat.ProductAccessEnabled)
        {
            records.Add(
                AuditRecord.Create(
                    correlationId,
                    actorUserId,
                    AuditAction.SeatUnassigned,
                    AuditTargetType.Seat,
                    seat.Id,
                    observedAt));
        }
        records.Add(
            AuditRecord.Create(
                correlationId,
                actorUserId,
                AuditAction.OrganizationMemberRemoved,
                AuditTargetType.OrganizationMembership,
                membership.Id,
                observedAt));
        return records.ToArray();
    }
}
