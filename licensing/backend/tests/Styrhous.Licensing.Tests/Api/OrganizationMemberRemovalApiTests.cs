using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationMemberRemovalApiTests
{
    private static readonly string[] ResponseProperties =
    [
        "reasonCode",
        "organizationId",
        "membershipId",
        "userId",
        "seatId",
        "correlationId",
        "removedAt",
    ];

    [Test]
    public async Task OwnerRemovesMemberThroughInternalApi()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "remove-api-owner", "owner@example.com");
        var member = await SignUpAsync(database, "remove-api-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal API Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var observedAt = SignupTime.AddDays(3);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await DeleteMemberAsync(
            client,
            organization.OrganizationId,
            memberSetup.MembershipId);
        var bodyText = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(bodyText);
        var correlationId = body.RootElement.GetProperty("correlationId").GetGuid();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(OrganizationReasonCodes.MemberRemoved));
            Assert.That(body.RootElement.GetProperty("organizationId").GetGuid(),
                Is.EqualTo(organization.OrganizationId));
            Assert.That(body.RootElement.GetProperty("membershipId").GetGuid(),
                Is.EqualTo(memberSetup.MembershipId));
            Assert.That(body.RootElement.GetProperty("userId").GetGuid(),
                Is.EqualTo(member.UserId));
            Assert.That(body.RootElement.GetProperty("seatId").GetGuid(),
                Is.EqualTo(memberSetup.SeatId));
            Assert.That(correlationId.Version, Is.EqualTo(7));
            Assert.That(body.RootElement.GetProperty("removedAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt));
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ResponseProperties));
            Assert.That(bodyText, Does.Not.Contain("device").IgnoreCase);
        });
        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                context.OrganizationMemberships.Any(
                    membership => membership.Id == memberSetup.MembershipId),
                Is.False);
            Assert.That(context.Seats.Any(seat => seat.Id == memberSetup.SeatId), Is.False);
            Assert.That(
                context.DeviceActivations.Any(
                    activation => activation.SeatId == memberSetup.SeatId),
                Is.False);
            Assert.That(
                context.AuditRecords.Count(record => record.CorrelationId == correlationId),
                Is.EqualTo(3));
        });
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public async Task NonOwnerCanLeaveThroughTheSameEndpoint(OrganizationRole role)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"leave-api-owner-{role}",
            $"owner-{role}@example.com");
        var member = await SignUpAsync(
            database,
            $"leave-api-member-{role}",
            $"member-{role}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Leave API Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            role);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(member.UserId);

        using var response = await DeleteMemberAsync(
            client,
            organization.OrganizationId,
            memberSetup.MembershipId);

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        await using var context = database.CreateContext();
        Assert.That(
            await context.OrganizationMemberships.AnyAsync(
                membership => membership.Id == memberSetup.MembershipId),
            Is.False);
    }

    [Test]
    public async Task UnauthenticatedStaleAndMissingAntiforgeryRequestsCannotRemoveMember()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "remove-auth-owner", "owner@example.com");
        var member = await SignUpAsync(database, "remove-auth-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Auth Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var baseline = await CaptureWritesAsync(database, memberSetup.MembershipId);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var anonymousClient = factory.CreateApiClient();
        using var staleClient = factory.CreateApiClient(Guid.CreateVersion7());
        using var ownerClient = factory.CreateApiClient(owner.UserId);

        using var anonymousResponse = await anonymousClient.DeleteAsync(
            MemberPath(organization.OrganizationId, memberSetup.MembershipId));
        using var staleResponse = await DeleteMemberAsync(
            staleClient,
            organization.OrganizationId,
            memberSetup.MembershipId);
        using var missingAntiforgeryResponse = await ownerClient.DeleteAsync(
            MemberPath(organization.OrganizationId, memberSetup.MembershipId));

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(missingAntiforgeryResponse.StatusCode,
                Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(staleResponse.Headers.CacheControl?.NoStore, Is.True);
        });
        AssertWritesUnchanged(
            baseline,
            await CaptureWritesAsync(database, memberSetup.MembershipId));
    }

    [Test]
    public async Task InvalidUnknownAndHiddenOrganizationsShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "remove-private-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "remove-private-outsider",
            "outsider@example.com");
        var member = await SignUpAsync(database, "remove-private-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Private Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);

        using var invalidResponse = await DeleteMemberAsync(
            ownerClient,
            Guid.NewGuid().ToString(),
            memberSetup.MembershipId.ToString());
        using var malformedResponse = await DeleteMemberAsync(
            ownerClient,
            TestIdentifiers.MalformedText,
            memberSetup.MembershipId.ToString());
        using var nonRfcResponse = await DeleteMemberAsync(
            ownerClient,
            TestIdentifiers.Version7WithNonRfcVariantText,
            memberSetup.MembershipId.ToString());
        using var unknownResponse = await DeleteMemberAsync(
            ownerClient,
            Guid.CreateVersion7(),
            memberSetup.MembershipId);
        using var hiddenResponse = await DeleteMemberAsync(
            outsiderClient,
            organization.OrganizationId,
            memberSetup.MembershipId);
        using var hiddenMalformedMemberResponse = await DeleteMemberAsync(
            outsiderClient,
            organization.OrganizationId.ToString(),
            TestIdentifiers.MalformedText);
        var responses = new[]
        {
            invalidResponse,
            malformedResponse,
            nonRfcResponse,
            unknownResponse,
            hiddenResponse,
            hiddenMalformedMemberResponse,
        };
        var reasonCodes = await Task.WhenAll(
            responses.Select(response => ReadReasonCodeAsync(response)));

        for (var index = 0; index < responses.Length; index++)
        {
            Assert.Multiple(() =>
            {
                Assert.That(responses[index].StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(responses[index].Headers.CacheControl?.NoStore, Is.True);
                Assert.That(reasonCodes[index],
                    Is.EqualTo(OrganizationReasonCodes.OrganizationNotFound));
            });
        }
    }

    [Test]
    public async Task InvalidUnknownCrossOrganizationAndRepeatedMembersShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "remove-member-owner", "owner@example.com");
        var member = await SignUpAsync(database, "remove-member-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Member Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Other Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var invalidResponse = await DeleteMemberAsync(
            client,
            organization.OrganizationId.ToString(),
            Guid.NewGuid().ToString());
        using var malformedResponse = await DeleteMemberAsync(
            client,
            organization.OrganizationId.ToString(),
            TestIdentifiers.MalformedText);
        using var nonRfcResponse = await DeleteMemberAsync(
            client,
            organization.OrganizationId.ToString(),
            TestIdentifiers.Version7WithNonRfcVariantText);
        using var unknownResponse = await DeleteMemberAsync(
            client,
            organization.OrganizationId,
            Guid.CreateVersion7());
        using var crossResponse = await DeleteMemberAsync(
            client,
            otherOrganization.OrganizationId,
            memberSetup.MembershipId);
        using var removedResponse = await DeleteMemberAsync(
            client,
            organization.OrganizationId,
            memberSetup.MembershipId);
        using var repeatedResponse = await DeleteMemberAsync(
            client,
            organization.OrganizationId,
            memberSetup.MembershipId);
        var notFoundResponses = new[]
        {
            invalidResponse,
            malformedResponse,
            nonRfcResponse,
            unknownResponse,
            crossResponse,
            repeatedResponse,
        };
        var reasonCodes = await Task.WhenAll(
            notFoundResponses.Select(response => ReadReasonCodeAsync(response)));

        Assert.That(removedResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        for (var index = 0; index < notFoundResponses.Length; index++)
        {
            Assert.Multiple(() =>
            {
                Assert.That(notFoundResponses[index].StatusCode,
                    Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(reasonCodes[index],
                    Is.EqualTo(OrganizationReasonCodes.MemberNotFound));
            });
        }
    }

    [Test]
    public async Task PermissionAndOwnershipFailuresMapToStableInternalErrors()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "remove-errors-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "remove-errors-admin", "admin@example.com");
        var member = await SignUpAsync(database, "remove-errors-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Errors Organization",
            SignupTime.AddDays(1));
        var adminSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await using var context = database.CreateContext();
        var ownerMembershipId = await context.OrganizationMemberships
            .Where(membership => membership.UserId == owner.UserId
                && membership.OrganizationId == organization.OrganizationId)
            .Select(membership => membership.Id)
            .SingleAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var memberClient = factory.CreateApiClient(member.UserId);
        using var adminClient = factory.CreateApiClient(admin.UserId);
        using var ownerClient = factory.CreateApiClient(owner.UserId);

        using var memberResponse = await DeleteMemberAsync(
            memberClient,
            organization.OrganizationId,
            adminSetup.MembershipId);
        using var adminResponse = await DeleteMemberAsync(
            adminClient,
            organization.OrganizationId,
            ownerMembershipId);
        using var ownerResponse = await DeleteMemberAsync(
            ownerClient,
            organization.OrganizationId,
            ownerMembershipId);
        var reasonCodes = await Task.WhenAll(
            ReadReasonCodeAsync(memberResponse),
            ReadReasonCodeAsync(adminResponse),
            ReadReasonCodeAsync(ownerResponse));

        Assert.Multiple(() =>
        {
            Assert.That(memberResponse.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(reasonCodes[0],
                Is.EqualTo(OrganizationReasonCodes.InsufficientPermission));
            Assert.That(adminResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(reasonCodes[1],
                Is.EqualTo(OrganizationReasonCodes.OwnershipTransferRequired));
            Assert.That(ownerResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(reasonCodes[2],
                Is.EqualTo(OrganizationReasonCodes.OwnershipTransferRequired));
        });
        await using var verificationContext = database.CreateContext();
        Assert.That(
            await verificationContext.OrganizationMemberships.CountAsync(),
            Is.EqualTo(3));
        Assert.That(memberSetup.MembershipId, Is.Not.EqualTo(adminSetup.MembershipId));
    }

    [Test]
    public async Task RepeatedMembershipDeleteConflictsReturnStableApiConflict()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "remove-race-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "remove-race-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Race Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var baseline = await CaptureWritesAsync(database, memberSetup.MembershipId);
        var interceptor = new OrganizationMemberRemovalConflictInterceptor();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3),
            useTestAuthentication: true,
            requestCompletionObserver: null,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await DeleteMemberAsync(
            client,
            organization.OrganizationId,
            memberSetup.MembershipId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(reasonCode, Is.EqualTo(OrganizationReasonCodes.MembersChanged));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(interceptor.SuppressedDeleteCount, Is.EqualTo(3));
        });
        AssertWritesUnchanged(
            baseline,
            await CaptureWritesAsync(database, memberSetup.MembershipId));
    }

    [Test]
    public async Task CancelledRequestRollsBackMembershipSeatDevicesAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "remove-cancel-owner", "owner@example.com");
        var member = await SignUpAsync(database, "remove-cancel-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Removal Cancellation Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            1,
            SignupTime.AddDays(2));
        var baseline = await CaptureWritesAsync(database, memberSetup.MembershipId);
        var gate = new DatabaseCommandGate();
        var saveGateInterceptor = new SavedChangesGateInterceptor(gate);
        var path = MemberPath(organization.OrganizationId, memberSetup.MembershipId);
        var completionObserver = new RequestCompletionObserver(HttpMethods.Delete, path);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3),
            useTestAuthentication: true,
            requestCompletionObserver: completionObserver,
            interceptors: [saveGateInterceptor]);
        using var client = factory.CreateApiClient(owner.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = DeleteMemberAsync(
            client,
            organization.OrganizationId,
            memberSetup.MembershipId,
            cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await responseTask);
        }
        finally
        {
            gate.Release();
        }

        await completionObserver.WaitUntilCompletedAsync();
        Assert.Multiple(() =>
        {
            Assert.That(completionObserver.WasCanceled, Is.True);
            Assert.That(saveGateInterceptor.CancellationObserved, Is.True);
        });
        AssertWritesUnchanged(
            baseline,
            await CaptureWritesAsync(database, memberSetup.MembershipId));
    }

    private static Task<HttpResponseMessage> DeleteMemberAsync(
        HttpClient client,
        Guid organizationId,
        Guid membershipId,
        CancellationToken cancellationToken = default)
    {
        return DeleteMemberAsync(
            client,
            organizationId.ToString(),
            membershipId.ToString(),
            cancellationToken);
    }

    private static async Task<HttpResponseMessage> DeleteMemberAsync(
        HttpClient client,
        string organizationId,
        string membershipId,
        CancellationToken cancellationToken = default)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(
            HttpMethod.Delete,
            MemberPath(organizationId, membershipId));
        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request, cancellationToken);
    }

    private static string MemberPath(Guid organizationId, Guid membershipId)
    {
        return MemberPath(organizationId.ToString(), membershipId.ToString());
    }

    private static string MemberPath(string organizationId, string membershipId)
    {
        return $"/api/organizations/{organizationId}/members/{membershipId}";
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        var content = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(content);
        return body.RootElement.GetProperty("reasonCode").GetString();
    }

    private static async Task<RemovalWriteSnapshot> CaptureWritesAsync(
        PostgresTestDatabase database,
        Guid membershipId)
    {
        await using var context = database.CreateContext();
        var membership = await context.OrganizationMemberships
            .AsNoTracking()
            .Where(candidate => candidate.Id == membershipId)
            .Select(candidate => new MembershipSnapshot(candidate.Id, candidate.UserId))
            .SingleOrDefaultAsync();
        var seatIds = membership is null
            ? []
            : await context.Seats
                .AsNoTracking()
                .Where(seat => seat.AssignedUserId == membership.UserId)
                .OrderBy(seat => seat.Id)
                .Select(seat => seat.Id)
                .ToArrayAsync();
        var deviceIds = await context.DeviceActivations
            .AsNoTracking()
            .Where(activation => seatIds.Contains(activation.SeatId))
            .OrderBy(activation => activation.Id)
            .Select(activation => activation.Id)
            .ToArrayAsync();
        return new RemovalWriteSnapshot(
            membership,
            seatIds,
            deviceIds,
            await context.AuditRecords.CountAsync());
    }

    private static void AssertWritesUnchanged(
        RemovalWriteSnapshot before,
        RemovalWriteSnapshot after)
    {
        Assert.Multiple(() =>
        {
            Assert.That(after.Membership, Is.EqualTo(before.Membership));
            Assert.That(after.SeatIds, Is.EqualTo(before.SeatIds));
            Assert.That(after.DeviceIds, Is.EqualTo(before.DeviceIds));
            Assert.That(after.AuditCount, Is.EqualTo(before.AuditCount));
        });
    }

    private sealed record RemovalWriteSnapshot(
        MembershipSnapshot? Membership,
        IReadOnlyList<Guid> SeatIds,
        IReadOnlyList<Guid> DeviceIds,
        int AuditCount);

    private sealed record MembershipSnapshot(Guid MembershipId, Guid UserId);
}
