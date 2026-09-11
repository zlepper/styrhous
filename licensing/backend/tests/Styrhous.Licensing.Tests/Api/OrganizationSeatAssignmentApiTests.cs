using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationSeatAssignmentApiTests
{
    private static readonly string[] ResponseProperties =
    [
        "reasonCode",
        "organizationId",
        "membershipId",
        "userId",
        "seatId",
        "assigned",
        "deviceLimit",
        "correlationId",
        "changedAt",
    ];

    [Test]
    public async Task OwnerAndAdminCanChangeAnyProductSeatWhileMemberCannot()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-api-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "seat-api-admin", "admin@example.com");
        var member = await SignUpAsync(database, "seat-api-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat API Organization",
            SignupTime.AddDays(1));
        var adminSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin,
            productAccessEnabled: false);
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var observedAt = SignupTime.AddDays(2);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var adminClient = factory.CreateApiClient(admin.UserId);
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var memberClient = factory.CreateApiClient(member.UserId);

        using var roster = await adminClient.GetAsync(
            $"/api/organizations/{organization.OrganizationId}/members");
        using var invitations = await adminClient.GetAsync(
            $"/api/organizations/{organization.OrganizationId}/invitations");
        using var disabled = await ChangeSeatAsync(
            adminClient,
            organization.OrganizationId,
            organization.OwnerMembershipId,
            assigned: false);
        var disabledText = await disabled.Content.ReadAsStringAsync();
        using var disabledBody = JsonDocument.Parse(disabledText);
        using var restored = await ChangeSeatAsync(
            ownerClient,
            organization.OrganizationId,
            organization.OwnerMembershipId,
            assigned: true);
        using var forbidden = await ChangeSeatAsync(
            memberClient,
            organization.OrganizationId,
            adminSetup.MembershipId,
            assigned: false);
        using var forbiddenMissing = await ChangeSeatAsync(
            memberClient,
            organization.OrganizationId,
            Guid.CreateVersion7(),
            assigned: false);
        var forbiddenMissingReason = await ReadReasonCodeAsync(forbiddenMissing);

        Assert.Multiple(() =>
        {
            Assert.That(roster.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(invitations.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(disabled.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(disabled.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                disabledBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(OrganizationReasonCodes.MemberSeatUnassigned));
            Assert.That(
                disabledBody.RootElement.GetProperty("organizationId").GetGuid(),
                Is.EqualTo(organization.OrganizationId));
            Assert.That(
                disabledBody.RootElement.GetProperty("membershipId").GetGuid(),
                Is.EqualTo(organization.OwnerMembershipId));
            Assert.That(disabledBody.RootElement.GetProperty("assigned").GetBoolean(), Is.False);
            Assert.That(
                disabledBody.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ResponseProperties));
            Assert.That(restored.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(forbidden.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(forbiddenMissing.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(
                forbiddenMissingReason,
                Is.EqualTo(OrganizationReasonCodes.InsufficientPermission));
        });

        await using var context = database.CreateContext();
        Assert.That(
            (await context.Seats.SingleAsync(item => item.Id == organization.SeatId))
                .ProductAccessEnabled,
            Is.True);
    }

    [Test]
    public async Task SeatMutationRequiresAssignmentStateAndAntiforgery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-validation-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat Validation Organization",
            SignupTime.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);
        var path = SeatPath(organization.OrganizationId, organization.OwnerMembershipId);

        using var missingToken = await client.PatchAsJsonAsync(path, new { assigned = false });
        var missingStateRequest = new HttpRequestMessage(HttpMethod.Patch, path)
        {
            Content = JsonContent.Create(new { }),
        };
        AntiforgeryTestClient.AddToken(
            missingStateRequest,
            await AntiforgeryTestClient.GetTokenAsync(client));
        using var missingState = await client.SendAsync(missingStateRequest);

        Assert.Multiple(() =>
        {
            Assert.That(missingToken.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(missingState.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        });
    }

    [Test]
    public async Task RejectionsPreservePrivacyAndDoNotChangeSeatOrAuditState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-errors-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "seat-errors-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat Error Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Other Seat Error Organization",
            SignupTime.AddDays(1));
        await using var baselineContext = database.CreateContext();
        var baselineAuditCount = await baselineContext.AuditRecords.CountAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);
        using var staleClient = factory.CreateApiClient(Guid.CreateVersion7());
        using var anonymousClient = factory.CreateApiClient();

        using var unchanged = await ChangeSeatAsync(
            ownerClient,
            organization.OrganizationId,
            organization.OwnerMembershipId,
            assigned: true);
        using var unknownMember = await ChangeSeatAsync(
            ownerClient,
            organization.OrganizationId,
            Guid.CreateVersion7(),
            assigned: false);
        using var malformedMember = await ChangeSeatAsync(
            ownerClient,
            organization.OrganizationId,
            "not-a-membership",
            assigned: false);
        using var crossOrganization = await ChangeSeatAsync(
            ownerClient,
            organization.OrganizationId,
            otherOrganization.OwnerMembershipId,
            assigned: false);
        using var hiddenOrganization = await ChangeSeatAsync(
            outsiderClient,
            organization.OrganizationId,
            organization.OwnerMembershipId,
            assigned: false);
        using var unknownOrganization = await ChangeSeatAsync(
            ownerClient,
            Guid.CreateVersion7(),
            organization.OwnerMembershipId,
            assigned: false);
        using var staleUser = await ChangeSeatAsync(
            staleClient,
            organization.OrganizationId,
            organization.OwnerMembershipId,
            assigned: false);
        using var anonymous = await anonymousClient.PatchAsJsonAsync(
            SeatPath(organization.OrganizationId, organization.OwnerMembershipId),
            new { assigned = false });
        var unchangedReason = await ReadReasonCodeAsync(unchanged);
        var unknownMemberReason = await ReadReasonCodeAsync(unknownMember);
        var malformedMemberReason = await ReadReasonCodeAsync(malformedMember);
        var crossOrganizationReason = await ReadReasonCodeAsync(crossOrganization);
        var hiddenOrganizationReason = await ReadReasonCodeAsync(hiddenOrganization);
        var unknownOrganizationReason = await ReadReasonCodeAsync(unknownOrganization);

        Assert.Multiple(() =>
        {
            Assert.That(unchanged.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(
                unchangedReason,
                Is.EqualTo(OrganizationReasonCodes.MemberSeatUnchanged));
            Assert.That(unknownMember.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                unknownMemberReason,
                Is.EqualTo(OrganizationReasonCodes.MemberNotFound));
            Assert.That(malformedMember.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                malformedMemberReason,
                Is.EqualTo(OrganizationReasonCodes.MemberNotFound));
            Assert.That(crossOrganization.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                crossOrganizationReason,
                Is.EqualTo(OrganizationReasonCodes.MemberNotFound));
            Assert.That(hiddenOrganization.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                hiddenOrganizationReason,
                Is.EqualTo(OrganizationReasonCodes.OrganizationNotFound));
            Assert.That(unknownOrganization.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(
                unknownOrganizationReason,
                Is.EqualTo(OrganizationReasonCodes.OrganizationNotFound));
            Assert.That(staleUser.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(anonymous.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
        });

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.Seats.Single(seat => seat.Id == organization.SeatId)
                    .ProductAccessEnabled,
                Is.True);
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task EnablingSeatReportsMissingAndExhaustedCapacityWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-capacity-owner", "owner@example.com");
        var trialOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Full Trial Organization",
            SignupTime.AddDays(1));
        for (var index = 0; index < 4; index++)
        {
            var activeMember = await SignUpAsync(
                database,
                $"seat-capacity-active-{index}",
                $"active-{index}@example.com");
            await AddOrganizationMemberAsync(
                database,
                trialOrganization,
                activeMember.UserId,
                OrganizationRole.Member);
        }
        var waitingMember = await SignUpAsync(
            database,
            "seat-capacity-waiting",
            "waiting@example.com");
        var waitingSeat = await AddOrganizationMemberAsync(
            database,
            trialOrganization,
            waitingMember.UserId,
            OrganizationRole.Member,
            productAccessEnabled: false);
        var unfundedOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Unfunded Organization",
            SignupTime.AddDays(1));
        var unfundedMember = await SignUpAsync(
            database,
            "seat-capacity-unfunded",
            "unfunded@example.com");
        var unfundedSeat = await AddOrganizationMemberAsync(
            database,
            unfundedOrganization,
            unfundedMember.UserId,
            OrganizationRole.Member,
            productAccessEnabled: false);
        await using var baselineContext = database.CreateContext();
        var baselineAuditCount = await baselineContext.AuditRecords.CountAsync();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var exhausted = await ChangeSeatAsync(
            client,
            trialOrganization.OrganizationId,
            waitingSeat.MembershipId,
            assigned: true);
        using var missing = await ChangeSeatAsync(
            client,
            unfundedOrganization.OrganizationId,
            unfundedSeat.MembershipId,
            assigned: true);
        var exhaustedReason = await ReadReasonCodeAsync(exhausted);
        var missingReason = await ReadReasonCodeAsync(missing);

        Assert.Multiple(() =>
        {
            Assert.That(exhausted.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(exhaustedReason, Is.EqualTo("seat_capacity_reached"));
            Assert.That(missing.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(missingReason, Is.EqualTo("no_active_seat_capacity"));
        });
        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.Seats.Single(seat => seat.Id == waitingSeat.SeatId)
                    .ProductAccessEnabled,
                Is.False);
            Assert.That(
                verificationContext.Seats.Single(seat => seat.Id == unfundedSeat.SeatId)
                    .ProductAccessEnabled,
                Is.False);
            Assert.That(
                verificationContext.AuditRecords.Count(),
                Is.EqualTo(baselineAuditCount));
        });
    }

    [TestCase("{}")]
    [TestCase("{\"assigned\":null}")]
    public async Task MissingAssignmentStateReturnsModelValidationWithoutChangingTheSeat(string body)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-validation-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database, owner.UserId, "Seat validation", SignupTime.AddDays(1));
        await using var baseline = database.CreateContext();
        var baselineAuditCount = await baseline.AuditRecords.CountAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);
        using var request = new HttpRequestMessage(
            HttpMethod.Patch, SeatPath(organization.OrganizationId, organization.OwnerMembershipId))
        {
            Content = new StringContent(body, System.Text.Encoding.UTF8, "application/json"),
        };
        AntiforgeryTestClient.AddToken(request, await AntiforgeryTestClient.GetTokenAsync(client));

        using var response = await client.SendAsync(request);
        using var problem = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(problem.RootElement.GetProperty("errors").TryGetProperty("assigned", out _), Is.True);
            Assert.That(verification.Seats.Single(seat => seat.Id == organization.SeatId).ProductAccessEnabled, Is.True);
            Assert.That(verification.AuditRecords.Count(), Is.EqualTo(baselineAuditCount));
        });
    }

    private static async Task<HttpResponseMessage> ChangeSeatAsync(
        HttpClient client,
        Guid organizationId,
        Guid membershipId,
        bool assigned)
    {
        return await ChangeSeatAsync(
            client,
            organizationId,
            membershipId.ToString(),
            assigned);
    }

    private static async Task<HttpResponseMessage> ChangeSeatAsync(
        HttpClient client,
        Guid organizationId,
        string membershipId,
        bool assigned)
    {
        var request = new HttpRequestMessage(
            HttpMethod.Patch,
            SeatPath(organizationId, membershipId))
        {
            Content = JsonContent.Create(new { assigned }),
        };
        AntiforgeryTestClient.AddToken(
            request,
            await AntiforgeryTestClient.GetTokenAsync(client));
        return await client.SendAsync(request);
    }

    private static string SeatPath(Guid organizationId, Guid membershipId)
    {
        return SeatPath(organizationId, membershipId.ToString());
    }

    private static string SeatPath(Guid organizationId, string membershipId)
    {
        return $"/api/organizations/{organizationId}/members/{membershipId}/seat";
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        return body.RootElement.GetProperty("reasonCode").GetString();
    }
}
