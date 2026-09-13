using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal enum OrganizationInvitationEligibilityStatus
{
    Eligible,
    AlreadyMember,
    InvitationAlreadyPending,
    NoActiveSeatCapacity,
    SeatCapacityReached,
}

internal sealed record OrganizationInvitationEligibilityResult(
    OrganizationInvitationEligibilityStatus Status,
    int? ReservedSeatCapacity)
{
    public static OrganizationInvitationEligibilityResult Eligible(int seatCapacity)
    {
        return new(OrganizationInvitationEligibilityStatus.Eligible, seatCapacity);
    }

    public static OrganizationInvitationEligibilityResult EligibleWithoutSeat()
    {
        return new(OrganizationInvitationEligibilityStatus.Eligible, ReservedSeatCapacity: null);
    }

    public static OrganizationInvitationEligibilityResult Rejected(
        OrganizationInvitationEligibilityStatus status)
    {
        return new(status, ReservedSeatCapacity: null);
    }
}

internal static class PostgresOrganizationInvitationEligibility
{
    public static async Task<OrganizationInvitationEligibilityResult> CheckAsync(
        LicensingDbContext dbContext,
        Organization organization,
        string normalizedEmail,
        DateTimeOffset observedAt,
        bool assignProductSeat,
        Guid? excludedInvitationId,
        int? preservedSeatCapacity,
        CancellationToken cancellationToken)
    {
        var invitedUserIds = dbContext.VerifiedEmailClaims
            .Where(claim => claim.NormalizedEmail == normalizedEmail)
            .Select(claim => claim.UserId);
        if (await dbContext.OrganizationMemberships.AnyAsync(
                membership => membership.OrganizationId == organization.Id
                    && invitedUserIds.Contains(membership.UserId),
                cancellationToken))
        {
            return OrganizationInvitationEligibilityResult.Rejected(
                OrganizationInvitationEligibilityStatus.AlreadyMember);
        }

        var activeInvitations = PostgresOrganizationInvitationQueries.Active(
                dbContext,
                observedAt,
                excludedInvitationId)
            .Where(invitation => invitation.OrganizationId == organization.Id);

        if (await activeInvitations.AnyAsync(
                invitation => invitation.NormalizedEmail == normalizedEmail,
                cancellationToken))
        {
            return OrganizationInvitationEligibilityResult.Rejected(
                OrganizationInvitationEligibilityStatus.InvitationAlreadyPending);
        }

        if (!assignProductSeat)
        {
            return OrganizationInvitationEligibilityResult.EligibleWithoutSeat();
        }

        var utcObservedAt = observedAt.ToUniversalTime();
        int? seatCapacity;
        if (preservedSeatCapacity is not null)
        {
            if (preservedSeatCapacity <= 0)
            {
                throw new InvalidOperationException(
                    "A persisted invitation reservation must have positive seat capacity.");
            }

            seatCapacity = await EfTransactionSerialization.TryClaimBillingAccountAsync(
                    dbContext,
                    organization.BillingAccountId,
                    cancellationToken)
                ? await PostgresOrganizationInvitationCapacity.ResolvePreservedCapacityAsync(
                    dbContext,
                    organization,
                    preservedSeatCapacity.Value,
                    utcObservedAt,
                    excludedInvitationId
                        ?? throw new InvalidOperationException(
                            "A preserved reservation must identify its invitation."),
                    cancellationToken)
                : null;
        }
        else
        {
            seatCapacity = await PostgresOrganizationSeatCapacity
                .SerializeBillingAccountAndResolveActiveAsync(
                dbContext,
                organization.BillingAccountId,
                utcObservedAt,
                cancellationToken);
        }

        if (seatCapacity is null)
        {
            return OrganizationInvitationEligibilityResult.Rejected(
                OrganizationInvitationEligibilityStatus.NoActiveSeatCapacity);
        }

        return await PostgresOrganizationInvitationCapacity.HasAvailableSeatAsync(
            dbContext,
            organization,
            seatCapacity.Value,
            utcObservedAt,
            excludedInvitationId,
            cancellationToken)
            ? OrganizationInvitationEligibilityResult.Eligible(seatCapacity.Value)
            : OrganizationInvitationEligibilityResult.Rejected(
                OrganizationInvitationEligibilityStatus.SeatCapacityReached);
    }
}
