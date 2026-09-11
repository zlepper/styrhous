using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationAcceptanceApiTests
{
    private const string AcceptancePath = "/api/invitations/accept";

    private static readonly string[] ResponseProperties =
    [
        "reasonCode",
        "invitationId",
        "organizationId",
        "membershipId",
        "seatId",
        "productSeatAssigned",
        "correlationId",
        "role",
        "acceptedAt",
    ];

    [TestCase(OrganizationRole.Admin, "admin", true)]
    [TestCase(OrganizationRole.Member, "member", true)]
    [TestCase(OrganizationRole.Member, "member", false)]
    public async Task InviteeAcceptsInvitationWithoutExposingSecret(
        OrganizationRole role,
        string expectedRole,
        bool assignProductSeat)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-api-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-api-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance API Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2),
            role,
            assignProductSeat);
        var observedAt = SignupTime.AddDays(3);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var client = factory.CreateApiClient(invitee.UserId);

        using var response = await PostAcceptanceAsync(
            client,
            invitation.Secret.Reveal());
        var bodyText = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(bodyText);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("invitation_accepted"));
            Assert.That(
                body.RootElement.GetProperty("invitationId").GetGuid(),
                Is.EqualTo(invitation.InvitationId));
            Assert.That(
                body.RootElement.GetProperty("organizationId").GetGuid(),
                Is.EqualTo(organization.OrganizationId));
            Assert.That(
                body.RootElement.GetProperty("membershipId").GetGuid().Version,
                Is.EqualTo(7));
            Assert.That(
                body.RootElement.GetProperty("seatId").GetGuid().Version,
                Is.EqualTo(7));
            Assert.That(
                body.RootElement.GetProperty("productSeatAssigned").GetBoolean(),
                Is.EqualTo(assignProductSeat));
            Assert.That(
                body.RootElement.GetProperty("correlationId").GetGuid().Version,
                Is.EqualTo(7));
            Assert.That(
                body.RootElement.GetProperty("role").GetString(),
                Is.EqualTo(expectedRole));
            Assert.That(
                body.RootElement.GetProperty("acceptedAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt));
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ResponseProperties));
            Assert.That(bodyText, Does.Not.Contain("secret").IgnoreCase);
            Assert.That(bodyText, Does.Not.Contain(invitation.Secret.Reveal()));
        });
    }

    [Test]
    public async Task UnauthenticatedAndStaleUsersCannotAcceptInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-auth-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-auth-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Auth Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var anonymousClient = factory.CreateApiClient();
        using var staleClient = factory.CreateApiClient(Guid.CreateVersion7());

        using var anonymousResponse = await anonymousClient.PostAsJsonAsync(
            AcceptancePath,
            new { secret = invitation.Secret.Reveal() });
        using var staleResponse = await PostAcceptanceAsync(
            staleClient,
            invitation.Secret.Reveal());

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(anonymousResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(staleResponse.Headers.CacheControl?.NoStore, Is.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task MissingAntiforgeryTokenRejectsAcceptanceWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-csrf-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-csrf-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance CSRF Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(invitee.UserId);

        using var response = await client.PostAsJsonAsync(
            AcceptancePath,
            new { secret = invitation.Secret.Reveal() });

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task InvalidSecretsReturnValidationProblemWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var invitee = await SignUpAsync(
            database,
            "accept-validation-invitee",
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(invitee.UserId);

        using var missingResponse = await PostAcceptanceContentAsync(
            client,
            JsonContent.Create(new { }));
        using var nullResponse = await PostAcceptanceAsync(client, secret: null);
        using var whitespaceResponse = await PostAcceptanceAsync(client, "   ");
        using var oversizedResponse = await PostAcceptanceAsync(
            client,
            new string('a', 257));
        using var malformedResponse = await PostAcceptanceContentAsync(
            client,
            new StringContent("{", System.Text.Encoding.UTF8, "application/json"));

        var responses = new[]
        {
            missingResponse,
            nullResponse,
            whitespaceResponse,
            oversizedResponse,
            malformedResponse,
        };
        Assert.Multiple(() =>
        {
            Assert.That(
                responses.Select(response => response.StatusCode),
                Is.All.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(
                responses.Select(response => response.Headers.CacheControl?.NoStore),
                Is.All.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task MaximumLengthSecretPassesValidation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var invitee = await SignUpAsync(
            database,
            "accept-maximum-secret-invitee",
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(invitee.UserId);

        using var response = await PostAcceptanceAsync(client, new string('a', 256));
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
            Assert.That(reasonCode, Is.EqualTo("invitation_not_found"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task UnknownAndTerminalSecretsShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-private-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-private-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Private Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
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
            SignupTime.AddDays(2));
        var accepted = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "accepted@example.com",
            SignupTime.AddDays(2));
        await using (var preparationContext = database.CreateContext())
        {
            await preparationContext.OrganizationInvitations
                .Where(invitation => invitation.Id == cancelled.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.CancelledAt,
                    SignupTime.AddDays(3)));
            await preparationContext.OrganizationInvitations
                .Where(invitation => invitation.Id == accepted.InvitationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(invitation => invitation.AcceptedAt, SignupTime.AddDays(3))
                    .SetProperty(invitation => invitation.AcceptedByUserId, owner.UserId));
        }

        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, expired.ExpiresAt);
        using var client = factory.CreateApiClient(invitee.UserId);
        using var unknownResponse = await PostAcceptanceAsync(client, "unknown-secret");
        using var expiredResponse = await PostAcceptanceAsync(
            client,
            expired.Secret.Reveal());
        using var cancelledResponse = await PostAcceptanceAsync(
            client,
            cancelled.Secret.Reveal());
        using var acceptedResponse = await PostAcceptanceAsync(
            client,
            accepted.Secret.Reveal());
        var responses = new[]
        {
            unknownResponse,
            expiredResponse,
            cancelledResponse,
            acceptedResponse,
        };
        var reasonCodes = await Task.WhenAll(responses.Select(ReadReasonCodeAsync));

        Assert.Multiple(() =>
        {
            Assert.That(
                responses.Select(response => response.StatusCode),
                Is.All.EqualTo(HttpStatusCode.NotFound));
            Assert.That(reasonCodes, Is.All.EqualTo("invitation_not_found"));
            Assert.That(
                responses.Select(response => response.Headers.CacheControl?.NoStore),
                Is.All.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task EmailMismatchReturnsForbiddenWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-email-api-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-email-api-invitee",
            "different@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Email API Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(invitee.UserId);

        using var response = await PostAcceptanceAsync(
            client,
            invitation.Secret.Reveal());
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(reasonCode, Is.EqualTo("invitation_email_mismatch"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task ExistingMemberReturnsConflictWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-member-api-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-member-api-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Member API Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await AddOrganizationMemberAsync(
            database,
            organization,
            invitee.UserId,
            OrganizationRole.Member);
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(invitee.UserId);

        using var response = await PostAcceptanceAsync(
            client,
            invitation.Secret.Reveal());
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(reasonCode, Is.EqualTo("already_member"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task CancelledRequestRollsBackInvitationMembershipSeatAndAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-request-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-request-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Request Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        var gate = new DatabaseCommandGate();
        var saveGateInterceptor = new SavedChangesGateInterceptor(gate);
        var completionObserver = new RequestCompletionObserver(HttpMethods.Post, AcceptancePath);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3),
            useTestAuthentication: true,
            requestCompletionObserver: completionObserver,
            interceptors: [saveGateInterceptor]);
        using var client = factory.CreateApiClient(invitee.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = PostAcceptanceAsync(
            client,
            invitation.Secret.Reveal(),
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
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    private static async Task<HttpResponseMessage> PostAcceptanceAsync(
        HttpClient client,
        string? secret,
        CancellationToken cancellationToken = default)
    {
        return await PostAcceptanceContentAsync(
            client,
            JsonContent.Create(new { secret }),
            cancellationToken);
    }

    private static async Task<HttpResponseMessage> PostAcceptanceContentAsync(
        HttpClient client,
        HttpContent content,
        CancellationToken cancellationToken = default)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(HttpMethod.Post, AcceptancePath)
        {
            Content = content,
        };
        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request, cancellationToken);
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        var content = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(content);
        return body.RootElement.GetProperty("reasonCode").GetString();
    }

    private static async Task<AcceptanceWriteSnapshot> CaptureWritesAsync(
        PostgresTestDatabase database)
    {
        await using var context = database.CreateContext();
        var invitations = await context.OrganizationInvitations
            .AsNoTracking()
            .OrderBy(invitation => invitation.Id)
            .Select(invitation => new InvitationSnapshot(
                invitation.Id,
                invitation.AcceptedAt,
                invitation.AcceptedByUserId))
            .ToArrayAsync();
        var memberships = await context.OrganizationMemberships
            .AsNoTracking()
            .OrderBy(membership => membership.Id)
            .Select(membership => new MembershipSnapshot(
                membership.Id,
                membership.OrganizationId,
                membership.UserId,
                membership.Role))
            .ToArrayAsync();
        var seats = await context.Seats
            .AsNoTracking()
            .OrderBy(seat => seat.Id)
            .Select(seat => new SeatSnapshot(
                seat.Id,
                seat.BillingAccountId,
                seat.AssignedUserId))
            .ToArrayAsync();
        var outboxMessages = await context.OutboxMessages
            .AsNoTracking()
            .OrderBy(message => message.Id)
            .Select(message => new OutboxMessageSnapshot(
                message.Id,
                message.SubjectId,
                message.NativeOutboxEnqueued,
                message.DiscardedAt,
                message.DiscardReason))
            .ToArrayAsync();
        var auditCount = await context.AuditRecords.CountAsync();
        return new AcceptanceWriteSnapshot(
            invitations,
            memberships,
            seats,
            outboxMessages,
            auditCount);
    }

    private static void AssertWritesUnchanged(
        AcceptanceWriteSnapshot before,
        AcceptanceWriteSnapshot after)
    {
        Assert.Multiple(() =>
        {
            Assert.That(after.Invitations, Is.EqualTo(before.Invitations));
            Assert.That(after.Memberships, Is.EqualTo(before.Memberships));
            Assert.That(after.Seats, Is.EqualTo(before.Seats));
            Assert.That(after.OutboxMessages, Is.EqualTo(before.OutboxMessages));
            Assert.That(after.AuditCount, Is.EqualTo(before.AuditCount));
        });
    }

    private sealed record AcceptanceWriteSnapshot(
        IReadOnlyList<InvitationSnapshot> Invitations,
        IReadOnlyList<MembershipSnapshot> Memberships,
        IReadOnlyList<SeatSnapshot> Seats,
        IReadOnlyList<OutboxMessageSnapshot> OutboxMessages,
        int AuditCount);

    private sealed record InvitationSnapshot(
        Guid InvitationId,
        DateTimeOffset? AcceptedAt,
        Guid? AcceptedByUserId);

    private sealed record MembershipSnapshot(
        Guid MembershipId,
        Guid OrganizationId,
        Guid UserId,
        OrganizationRole Role);

    private sealed record SeatSnapshot(
        Guid SeatId,
        Guid BillingAccountId,
        Guid AssignedUserId);

    private sealed record OutboxMessageSnapshot(
        Guid MessageId,
        Guid SubjectId,
        bool NativeOutboxEnqueued,
        DateTimeOffset? DiscardedAt,
        OutboxDiscardReason? DiscardReason);
}
