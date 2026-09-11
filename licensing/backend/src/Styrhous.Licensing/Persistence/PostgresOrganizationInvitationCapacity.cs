using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresOrganizationInvitationCapacity
{
    public static async Task ReserveEligibleInvitationAsync(
        LicensingDbContext dbContext,
        Organization organization,
        OrganizationInvitation invitation,
        int? reservedSeatCapacity,
        DateTimeOffset observedAt,
        Guid? excludedInvitationId,
        CancellationToken cancellationToken)
    {
        if (!invitation.AssignProductSeat)
        {
            if (reservedSeatCapacity is not null)
            {
                throw new InvalidOperationException(
                    "A seatless invitation cannot receive reserved seat capacity.");
            }

            return;
        }

        if (reservedSeatCapacity is null)
        {
            throw new InvalidOperationException(
                "An eligible seat-bearing invitation must receive reserved capacity.");
        }

        invitation.ReserveSeatCapacity(reservedSeatCapacity.Value);
        await PromoteActiveReservationsAsync(
            dbContext,
            organization,
            invitation.ReservedSeatCapacity,
            observedAt,
            excludedInvitationId,
            cancellationToken);
    }

    public static async Task<int> ResolvePreservedCapacityAsync(
        LicensingDbContext dbContext,
        Organization organization,
        int preservedSeatCapacity,
        DateTimeOffset observedAt,
        Guid excludedInvitationId,
        CancellationToken cancellationToken)
    {
        var largestOtherReservation = await PostgresOrganizationInvitationQueries.Active(
                dbContext,
                observedAt,
                excludedInvitationId)
            .Where(invitation => invitation.OrganizationId == organization.Id)
            .Where(invitation => invitation.AssignProductSeat)
            .MaxAsync(
                invitation => (int?)invitation.ReservedSeatCapacity,
                cancellationToken);
        return Math.Max(preservedSeatCapacity, largestOtherReservation ?? 0);
    }

    public static async Task PromoteActiveReservationsAsync(
        LicensingDbContext dbContext,
        Organization organization,
        int seatCapacity,
        DateTimeOffset observedAt,
        Guid? excludedInvitationId,
        CancellationToken cancellationToken)
    {
        var invitations = await PostgresOrganizationInvitationQueries.Active(
                dbContext,
                observedAt,
                excludedInvitationId)
            .Where(invitation => invitation.OrganizationId == organization.Id
                && invitation.AssignProductSeat
                && invitation.ReservedSeatCapacity < seatCapacity)
            .ToListAsync(cancellationToken);
        foreach (var invitation in invitations)
        {
            invitation.ReserveSeatCapacity(seatCapacity);
        }
    }

    public static async Task<bool> HasAvailableSeatAsync(
        LicensingDbContext dbContext,
        Organization organization,
        int seatCapacity,
        DateTimeOffset observedAt,
        Guid? excludedInvitationId,
        CancellationToken cancellationToken)
    {
        var requiredSeatCount = await RequiredSeatCountAsync(
            dbContext,
            organization.Id,
            organization.BillingAccountId,
            observedAt,
            excludedInvitationId,
            cancellationToken);
        return requiredSeatCount < seatCapacity;
    }

    public static async Task<int> RequiredSeatCountAsync(
        LicensingDbContext dbContext,
        Guid organizationId,
        Guid billingAccountId,
        DateTimeOffset observedAt,
        Guid? excludedInvitationId,
        CancellationToken cancellationToken)
    {
        var assignedSeatCount = await dbContext.Seats.CountAsync(
            seat => seat.BillingAccountId == billingAccountId
                && seat.ProductAccessEnabled,
            cancellationToken);
        var activeInvitationCount = await PostgresOrganizationInvitationQueries.Active(
                dbContext,
                observedAt,
                excludedInvitationId)
            .Where(invitation => invitation.OrganizationId == organizationId)
            .Where(invitation => invitation.AssignProductSeat)
            .CountAsync(cancellationToken);
        return checked(assignedSeatCount + activeInvitationCount);
    }
}
