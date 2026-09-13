using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationListingPersistenceTests
{
    private static readonly string[] ActiveEmails =
    [
        "first-active@example.com",
        "second-active@example.com",
    ];

    [Test]
    public async Task OwnersAndAdminsListOnlyActiveTargetInvitationsInCreationOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-list-owner",
            "owner@example.com");
        var admin = await SignUpAsync(
            database,
            "invitation-list-admin",
            "admin@example.com");
        var member = await SignUpAsync(
            database,
            "invitation-list-member",
            "member@example.com");
        var acceptedUser = await SignUpAsync(
            database,
            "invitation-list-accepted",
            "accepted@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Listing Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin,
            joinedAt: SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(1));

        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired@example.com",
            SignupTime.AddDays(2));
        var cancelled = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "cancelled@example.com",
            SignupTime.AddDays(9));
        await MarkCancelledAsync(
            database,
            cancelled.InvitationId,
            SignupTime.AddDays(9).AddHours(6));
        var accepted = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "accepted@example.com",
            SignupTime.AddDays(9).AddHours(12));
        await MarkAcceptedAsync(
            database,
            accepted.InvitationId,
            acceptedUser.UserId,
            SignupTime.AddDays(9).AddHours(18));
        var firstActive = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "first-active@example.com",
            SignupTime.AddDays(10),
            OrganizationRole.Admin);
        var secondActive = await CreateInvitationAsync(
            database,
            admin.UserId,
            organization.OrganizationId,
            "second-active@example.com",
            SignupTime.AddDays(11));
        var firstResentAt = SignupTime.AddDays(11).AddHours(12);
        await ResendInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            firstActive.InvitationId,
            firstResentAt);
        var otherOrganization = await CreateOrganizationAsync(
            database,
            admin.UserId,
            "Other Invitation Listing Organization",
            SignupTime.AddDays(7));
        var otherInvitation = await CreateInvitationAsync(
            database,
            admin.UserId,
            otherOrganization.OrganizationId,
            "other-organization@example.com",
            SignupTime.AddDays(10));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationListingService>.ForDatabase(
            database, SignupTime.AddDays(12));
        var service = serviceTest.Service;

        var ownerResult = RequireSuccess(
            await service.ListAsync(owner.UserId, organization.OrganizationId));
        var adminResult = RequireSuccess(
            await service.ListAsync(admin.UserId, organization.OrganizationId));

        Assert.Multiple(() =>
        {
            Assert.That(ownerResult.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(
                ownerResult.Invitations.Select(invitation => invitation.InvitationId),
                Is.EqualTo(new[]
                {
                    firstActive.InvitationId,
                    secondActive.InvitationId,
                }));
            Assert.That(
                adminResult.Invitations,
                Is.EqualTo(ownerResult.Invitations));
            Assert.That(
                ownerResult.Invitations.Select(invitation => invitation.InvitationId),
                Does.Not.Contain(otherInvitation.InvitationId));
            Assert.That(
                ownerResult.Invitations.Select(invitation => invitation.Email),
                Is.EquivalentTo(ActiveEmails));
        });
        AssertInvitation(
            ownerResult.Invitations[0],
            firstActive.InvitationId,
            owner.UserId,
            "first-active@example.com",
            OrganizationRole.Admin,
            SignupTime.AddDays(10),
            firstResentAt);
        AssertInvitation(
            ownerResult.Invitations[1],
            secondActive.InvitationId,
            admin.UserId,
            "second-active@example.com",
            OrganizationRole.Member,
            SignupTime.AddDays(11));
    }

    [Test]
    public async Task MemberIsForbiddenWhileNonMemberAndUnknownOrganizationAreHidden()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-private-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "invitation-private-member",
            "member@example.com");
        var outsider = await SignUpAsync(
            database,
            "invitation-private-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Private Invitation Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await CreateOrganizationAsync(
            database,
            outsider.UserId,
            "Outsider Invitation Organization",
            SignupTime.AddDays(1));
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "private-invitation@example.com",
            SignupTime.AddDays(2));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var service = serviceTest.Service;

        var memberResult = await service.ListAsync(
            member.UserId,
            organization.OrganizationId);
        var hiddenResult = await service.ListAsync(
            outsider.UserId,
            organization.OrganizationId);
        var unknownResult = await service.ListAsync(
            owner.UserId,
            Guid.CreateVersion7());

        Assert.Multiple(() =>
        {
            Assert.That(
                memberResult.Status,
                Is.EqualTo(OrganizationInvitationListingStatus.InsufficientPermission));
            Assert.That(
                hiddenResult.Status,
                Is.EqualTo(OrganizationInvitationListingStatus.OrganizationNotFound));
            Assert.That(
                unknownResult.Status,
                Is.EqualTo(OrganizationInvitationListingStatus.OrganizationNotFound));
            Assert.That(memberResult, Is.TypeOf<OrganizationInvitationListingResult.Rejection>());
            Assert.That(hiddenResult, Is.TypeOf<OrganizationInvitationListingResult.Rejection>());
            Assert.That(unknownResult, Is.TypeOf<OrganizationInvitationListingResult.Rejection>());
        });
    }

    [Test]
    public async Task InvitationExpiresAtTheListingBoundary()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-expiry-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Expiry Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "boundary@example.com",
            SignupTime.AddDays(2));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationListingService>.ForDatabase(
            database, invitation.ExpiresAt);
        var service = serviceTest.Service;

        var result = RequireSuccess(
            await service.ListAsync(owner.UserId, organization.OrganizationId));

        Assert.That(result.Invitations, Is.Empty);
    }

    [Test]
    public async Task EqualCreationTimesUseInvitationIdentifierTieBreak()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-order-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Order Organization",
            SignupTime.AddDays(1));
        var createdAt = SignupTime.AddDays(2);
        var higherInvitationId = Guid.Parse("018f0000-0000-7000-8000-000000000002");
        var lowerInvitationId = Guid.Parse("018f0000-0000-7000-8000-000000000001");
        var firstCreated = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "order-first@example.com",
            createdAt);
        await ReplaceInvitationIdAsync(
            database,
            firstCreated.InvitationId,
            higherInvitationId);
        var secondCreated = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "order-second@example.com",
            createdAt);
        await ReplaceInvitationIdAsync(
            database,
            secondCreated.InvitationId,
            lowerInvitationId);
        await using var serviceTest = ServiceTestBase<OrganizationInvitationListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var service = serviceTest.Service;

        var result = RequireSuccess(
            await service.ListAsync(owner.UserId, organization.OrganizationId));

        Assert.That(
            result.Invitations.Select(invitation => invitation.InvitationId),
            Is.EqualTo(new[] { lowerInvitationId, higherInvitationId }));
    }

    [Test]
    public async Task UnknownActorCannotListOrganizationInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var serviceTest = ServiceTestBase<OrganizationInvitationListingService>.ForDatabase(
            database, SignupTime);
        var service = serviceTest.Service;

        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await service.ListAsync(
                Guid.CreateVersion7(),
                Guid.CreateVersion7()));
    }

    private static async Task MarkCancelledAsync(
        PostgresTestDatabase database,
        Guid invitationId,
        DateTimeOffset cancelledAt)
    {
        await using var context = database.CreateContext();
        await context.OrganizationInvitations
            .Where(invitation => invitation.Id == invitationId)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    invitation => invitation.CancelledAt,
                    cancelledAt));
    }

    private static async Task MarkAcceptedAsync(
        PostgresTestDatabase database,
        Guid invitationId,
        Guid acceptedByUserId,
        DateTimeOffset acceptedAt)
    {
        await using var context = database.CreateContext();
        await context.OrganizationInvitations
            .Where(invitation => invitation.Id == invitationId)
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(invitation => invitation.AcceptedAt, acceptedAt)
                    .SetProperty(invitation => invitation.AcceptedByUserId, acceptedByUserId));
    }

    private static async Task ReplaceInvitationIdAsync(
        PostgresTestDatabase database,
        Guid invitationId,
        Guid replacementId)
    {
        await using var context = database.CreateContext();
        await context.OrganizationInvitations
            .Where(invitation => invitation.Id == invitationId)
            .ExecuteUpdateAsync(setters => setters.SetProperty(
                invitation => invitation.Id,
                replacementId));
        await context.AuditRecords
            .Where(record => record.TargetId == invitationId)
            .ExecuteUpdateAsync(setters => setters.SetProperty(
                record => record.TargetId,
                replacementId));
    }

    private static OrganizationInvitationListingResult.Success RequireSuccess(
        OrganizationInvitationListingResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationInvitationListingResult.Success>());
        return (OrganizationInvitationListingResult.Success)result;
    }

    private static void AssertInvitation(
        OrganizationInvitationSummary invitation,
        Guid invitationId,
        Guid createdByUserId,
        string email,
        OrganizationRole role,
        DateTimeOffset createdAt,
        DateTimeOffset? lastSentAt = null)
    {
        var expectedLastSentAt = lastSentAt ?? createdAt;
        Assert.Multiple(() =>
        {
            Assert.That(invitation.InvitationId, Is.EqualTo(invitationId));
            Assert.That(invitation.CreatedByUserId, Is.EqualTo(createdByUserId));
            Assert.That(invitation.Email, Is.EqualTo(email));
            Assert.That(invitation.Role, Is.EqualTo(role));
            Assert.That(invitation.CreatedAt, Is.EqualTo(createdAt));
            Assert.That(invitation.LastSentAt, Is.EqualTo(expectedLastSentAt));
            Assert.That(
                invitation.ExpiresAt,
                Is.EqualTo(expectedLastSentAt.Add(OrganizationInvitation.Lifetime)));
            Assert.That(invitation.InvitationId.Version, Is.EqualTo(7));
            Assert.That(invitation.CreatedByUserId.Version, Is.EqualTo(7));
        });
    }
}
