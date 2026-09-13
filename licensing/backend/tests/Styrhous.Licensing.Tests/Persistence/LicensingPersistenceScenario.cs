using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

internal static class LicensingPersistenceScenario
{
    public static readonly DateTimeOffset SignupTime =
        new(2026, 8, 30, 10, 15, 0, TimeSpan.Zero);

    public static async Task<SignupResult> SignUpAsync(
        PostgresTestDatabase database,
        string subject = "licensing-user",
        string email = "person@example.com")
    {
        await using var test = ServiceTestBase<UserSignupService>.ForDatabase(database, SignupTime);
        return await test.Service
            .SignUpAsync(VerifiedExternalIdentity.Create("github", subject, email));
    }

    public static async Task<OrganizationCreationResult> CreateOrganizationAsync(
        PostgresTestDatabase database,
        Guid userId,
        string name,
        DateTimeOffset observedAt)
    {
        await using var test = ServiceTestBase<OrganizationCreationService>.ForDatabase(database, observedAt);
        return await test.Service
            .CreateAsync(userId, name);
    }

    public static async Task<OrganizationMemberSetup> AddOrganizationMemberAsync(
        PostgresTestDatabase database,
        OrganizationCreationResult organization,
        Guid userId,
        OrganizationRole role,
        Guid? membershipId = null,
        Guid? seatId = null,
        int deviceLimit = Seat.DefaultDeviceLimit,
        DateTimeOffset? joinedAt = null,
        bool productAccessEnabled = true)
    {
        var resolvedMembershipId = membershipId ?? Guid.CreateVersion7();
        var resolvedSeatId = seatId ?? Guid.CreateVersion7();
        var resolvedJoinedAt = joinedAt ?? SignupTime.AddDays(1);
        await using var context = database.CreateContext();
        var membership = OrganizationMembership.AcceptInvitation(
            organization.OrganizationId,
            userId,
            OrganizationRole.Member,
            resolvedJoinedAt);
        var seat = Seat.Assign(
            organization.BillingAccountId,
            userId,
            resolvedJoinedAt,
            productAccessEnabled);
        context.Entry(membership).Property(item => item.Id).CurrentValue = resolvedMembershipId;
        context.Entry(membership).Property(item => item.Role).CurrentValue = role;
        context.Entry(seat).Property(item => item.Id).CurrentValue = resolvedSeatId;
        context.Entry(seat).Property(item => item.DeviceLimit).CurrentValue = deviceLimit;
        context.AddRange(membership, seat);
        await context.SaveChangesAsync();
        return new OrganizationMemberSetup(
            resolvedMembershipId,
            resolvedSeatId,
            resolvedJoinedAt);
    }

    public sealed record OrganizationMemberSetup(
        Guid MembershipId,
        Guid SeatId,
        DateTimeOffset JoinedAt);

    public static async Task<OrganizationInvitationCreationResult.Success> CreateInvitationAsync(
        PostgresTestDatabase database,
        Guid actorUserId,
        Guid organizationId,
        string email,
        DateTimeOffset? observedAt = null,
        OrganizationRole role = OrganizationRole.Member,
        bool assignProductSeat = true)
    {
        await using var test = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, observedAt ?? SignupTime.AddDays(2));
        var result = await test.Service
            .CreateAsync(
                actorUserId,
                organizationId,
                email,
                role,
                assignProductSeat);
        return result as OrganizationInvitationCreationResult.Success
            ?? throw new InvalidOperationException($"Invitation setup failed with {result.Status}.");
    }

    public static async Task<OrganizationInvitationResendResult.Success> ResendInvitationAsync(
        PostgresTestDatabase database,
        Guid actorUserId,
        Guid organizationId,
        Guid invitationId,
        DateTimeOffset observedAt)
    {
        await using var test = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(database, observedAt);
        var result = await test.Service
            .ResendAsync(actorUserId, organizationId, invitationId);
        return result as OrganizationInvitationResendResult.Success
            ?? throw new InvalidOperationException(
                $"Invitation resend setup failed with {result.Status}.");
    }
}
