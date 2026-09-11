using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationMemberListingPersistenceTests
{
    [Test]
    public async Task MemberListsCompleteRosterInMembershipOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-list-owner",
            "owner@example.com");
        var admin = await SignUpAsync(
            database,
            "member-list-admin",
            "admin@example.com");
        var member = await SignUpAsync(
            database,
            "member-list-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Member Listing Organization",
            SignupTime.AddDays(1));
        var adminSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin,
            membershipId: Guid.CreateVersion7(),
            seatId: Guid.CreateVersion7(),
            deviceLimit: 5,
            joinedAt: SignupTime.AddDays(2));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            membershipId: Guid.CreateVersion7(),
            seatId: Guid.CreateVersion7(),
            deviceLimit: 7,
            joinedAt: SignupTime.AddDays(3));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            admin.UserId,
            "Other Member Listing Organization",
            SignupTime.AddDays(4));
        var pendingInvitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "pending-roster@example.com",
            SignupTime.AddDays(5));
        await using var serviceTest = ServiceTestBase<OrganizationMemberListingService>.ForDatabase(
            database, SignupTime.AddDays(6));
        var service = serviceTest.Service;

        var result = await service.ListAsync(
            member.UserId,
            organization.OrganizationId);
        var success = RequireSuccess(result);

        Assert.That(success.OrganizationId, Is.EqualTo(organization.OrganizationId));
        Assert.That(
            success.Members.Select(candidate => candidate.MembershipId),
            Is.EqualTo(new[]
            {
                organization.OwnerMembershipId,
                adminSetup.MembershipId,
                memberSetup.MembershipId,
            }));
        Assert.That(
            success.Members.Select(candidate => candidate.MembershipId),
            Does.Not.Contain(otherOrganization.OwnerMembershipId));
        Assert.Multiple(() =>
        {
            Assert.That(
                success.Members.Select(candidate => candidate.MembershipId),
                Does.Not.Contain(pendingInvitation.InvitationId));
            Assert.That(
                success.Members.Select(candidate => candidate.Email),
                Does.Not.Contain("pending-roster@example.com"));
        });
        AssertMember(
            success.Members[0],
            organization.OwnerMembershipId,
            owner.UserId,
            organization.SeatId,
            "owner@example.com",
            OrganizationRole.Owner,
            deviceLimit: 3,
            SignupTime.AddDays(1));
        AssertMember(
            success.Members[1],
            adminSetup.MembershipId,
            admin.UserId,
            adminSetup.SeatId,
            "admin@example.com",
            OrganizationRole.Admin,
            deviceLimit: 5,
            SignupTime.AddDays(2));
        AssertMember(
            success.Members[2],
            memberSetup.MembershipId,
            member.UserId,
            memberSetup.SeatId,
            "member@example.com",
            OrganizationRole.Member,
            deviceLimit: 7,
            SignupTime.AddDays(3));
    }

    [Test]
    public async Task NonMemberAndUnknownOrganizationShareNotFoundStatus()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-private-owner",
            "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "member-private-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Private Member Organization",
            SignupTime.AddDays(1));
        await CreateOrganizationAsync(
            database,
            outsider.UserId,
            "Outsider Organization",
            SignupTime.AddDays(2));
        await using var serviceTest = ServiceTestBase<OrganizationMemberListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var service = serviceTest.Service;

        var hiddenResult = await service.ListAsync(
            outsider.UserId,
            organization.OrganizationId);
        var unknownResult = await service.ListAsync(
            outsider.UserId,
            Guid.CreateVersion7());

        Assert.Multiple(() =>
        {
            Assert.That(
                hiddenResult.Status,
                Is.EqualTo(OrganizationMemberListingStatus.OrganizationNotFound));
            Assert.That(
                unknownResult.Status,
                Is.EqualTo(OrganizationMemberListingStatus.OrganizationNotFound));
            Assert.That(hiddenResult, Is.TypeOf<OrganizationMemberListingResult.Rejection>());
            Assert.That(unknownResult, Is.TypeOf<OrganizationMemberListingResult.Rejection>());
        });
    }

    [Test]
    public async Task EqualJoinTimesUseMembershipIdentifierTieBreak()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-order-owner",
            "owner@example.com");
        var firstMember = await SignUpAsync(
            database,
            "member-order-first",
            "first@example.com");
        var secondMember = await SignUpAsync(
            database,
            "member-order-second",
            "second@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Ordered Member Organization",
            SignupTime.AddDays(1));
        var joinedAt = SignupTime.AddDays(2);
        var lowerMembershipId = Guid.Parse("018f0000-0000-7000-8000-000000000001");
        var higherMembershipId = Guid.Parse("018f0000-0000-7000-8000-000000000002");
        await AddOrganizationMemberAsync(
            database,
            organization,
            firstMember.UserId,
            OrganizationRole.Member,
            membershipId: higherMembershipId,
            seatId: Guid.CreateVersion7(),
            deviceLimit: 3,
            joinedAt: joinedAt);
        await AddOrganizationMemberAsync(
            database,
            organization,
            secondMember.UserId,
            OrganizationRole.Member,
            membershipId: lowerMembershipId,
            seatId: Guid.CreateVersion7(),
            deviceLimit: 3,
            joinedAt: joinedAt);
        await using var serviceTest = ServiceTestBase<OrganizationMemberListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var service = serviceTest.Service;

        var result = RequireSuccess(
            await service.ListAsync(owner.UserId, organization.OrganizationId));

        Assert.That(
            result.Members.Select(member => member.MembershipId),
            Is.EqualTo(new[]
            {
                organization.OwnerMembershipId,
                lowerMembershipId,
                higherMembershipId,
            }));
    }

    [Test]
    public async Task RosterUsesCurrentVerifiedEmailRatherThanHistoricalClaim()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-email-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "member-email-member",
            "historical@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Member Email Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await using (var updateContext = database.CreateContext())
        {
            await updateContext.UserAccounts
                .Where(user => user.Id == member.UserId)
                .ExecuteUpdateAsync(
                    setters => setters
                        .SetProperty(user => user.VerifiedEmail, "current@example.com")
                        .SetProperty(user => user.NormalizedEmail, "CURRENT@EXAMPLE.COM"));
        }

        await using var serviceTest = ServiceTestBase<OrganizationMemberListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var result = RequireSuccess(
            await serviceTest.Service.ListAsync(owner.UserId, organization.OrganizationId));

        Assert.That(
            result.Members.Single(candidate => candidate.UserId == member.UserId).Email,
            Is.EqualTo("current@example.com"));
    }

    [Test]
    public async Task UnknownActorCannotListOrganizationMembers()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var serviceTest = ServiceTestBase<OrganizationMemberListingService>.ForDatabase(
            database, SignupTime);
        var service = serviceTest.Service;

        Assert.ThrowsAsync<Styrhous.Licensing.Application.Accounts.UserNotFoundException>(
            async () => await service.ListAsync(
                Guid.CreateVersion7(),
                Guid.CreateVersion7()));
    }

    [Test]
    public async Task MembershipWithoutAssignedSeatFailsRatherThanReturningPartialRoster()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-invariant-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "member-invariant-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Broken Member Organization",
            SignupTime.AddDays(1));
        await using (var setupContext = database.CreateContext())
        {
            setupContext.OrganizationMemberships.Add(
                OrganizationMembership.AcceptInvitation(
                    organization.OrganizationId,
                    member.UserId,
                    OrganizationRole.Member,
                    SignupTime.AddDays(2)));
            await setupContext.SaveChangesAsync();
        }

        await using var serviceTest = ServiceTestBase<OrganizationMemberListingService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var service = serviceTest.Service;

        var exception = Assert.ThrowsAsync<InvalidOperationException>(
            async () => await service.ListAsync(
                owner.UserId,
                organization.OrganizationId));

        Assert.That(
            exception!.Message,
            Is.EqualTo("An organization membership is missing its assigned seat."));
    }

    private static OrganizationMemberListingResult.Success RequireSuccess(
        OrganizationMemberListingResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationMemberListingResult.Success>());
        return (OrganizationMemberListingResult.Success)result;
    }

    private static void AssertMember(
        OrganizationMemberSummary member,
        Guid membershipId,
        Guid userId,
        Guid seatId,
        string email,
        OrganizationRole role,
        int deviceLimit,
        DateTimeOffset joinedAt)
    {
        Assert.Multiple(() =>
        {
            Assert.That(member.MembershipId, Is.EqualTo(membershipId));
            Assert.That(member.UserId, Is.EqualTo(userId));
            Assert.That(member.SeatId, Is.EqualTo(seatId));
            Assert.That(member.Email, Is.EqualTo(email));
            Assert.That(member.Role, Is.EqualTo(role));
            Assert.That(member.ProductSeatAssigned, Is.True);
            Assert.That(member.DeviceLimit, Is.EqualTo(deviceLimit));
            Assert.That(member.JoinedAt, Is.EqualTo(joinedAt));
            Assert.That(
                new[] { member.MembershipId, member.UserId, member.SeatId },
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
        });
    }
}
