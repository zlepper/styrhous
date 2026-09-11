using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationResendApiTests
{
    private static readonly string[] ResponseProperties =
    [
        "reasonCode",
        "invitationId",
        "correlationId",
        "expiresAt",
    ];

    [Test]
    public async Task OwnerResendsInvitationWithoutExposingRotatedSecret()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-api-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend API Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var observedAt = SignupTime.AddDays(3);
        using var factory = new LicensingWebApplicationFactory(
            database,
            observedAt);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostResendAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        var bodyText = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(bodyText);
        var correlationId = body.RootElement.GetProperty("correlationId").GetGuid();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(OrganizationInvitationReasonCodes.Resent));
            Assert.That(body.RootElement.GetProperty("invitationId").GetGuid(),
                Is.EqualTo(invitation.InvitationId));
            Assert.That(correlationId.Version, Is.EqualTo(7));
            Assert.That(body.RootElement.GetProperty("expiresAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt.AddDays(7)));
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ResponseProperties));
            Assert.That(bodyText, Does.Not.Contain("secret").IgnoreCase);
        });
        await using var context = database.CreateContext();
        var persisted = await context.OrganizationInvitations.SingleAsync();
        var auditCount = await context.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationResent
                && record.CorrelationId == correlationId);
        var outbox = await context.OutboxMessages.SingleAsync(
            message => message.CorrelationId == correlationId);
        var supersededOutbox = await context.OutboxMessages.SingleAsync(
            message => message.SubjectId == invitation.InvitationId
                && message.CorrelationId != correlationId);
        var queued = await context.Set<RebusOutboxMessage>()
            .Where(message => message.DestinationAddress == database.DatabaseName)
            .ToArrayAsync();
        var queuedWorkIds = queued.Select(message =>
        {
            using var payload = JsonDocument.Parse(message.Body!);
            return payload.RootElement.GetProperty("WorkId").GetGuid();
        }).ToArray();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.SecretHash, Is.Not.EqualTo(invitation.Secret.Hash));
            Assert.That(persisted.LastSentAt, Is.EqualTo(observedAt));
            Assert.That(persisted.ExpiresAt, Is.EqualTo(observedAt.AddDays(7)));
            Assert.That(auditCount, Is.EqualTo(1));
            Assert.That(outbox.SubjectId, Is.EqualTo(invitation.InvitationId));
            Assert.That(outbox.NotAfter, Is.EqualTo(observedAt.AddDays(7)));
            Assert.That(outbox.NativeOutboxEnqueued, Is.True);
            Assert.That(outbox.DiscardedAt, Is.Null);
            Assert.That(queuedWorkIds, Is.EquivalentTo(new[] { supersededOutbox.Id, outbox.Id }));
            Assert.That(supersededOutbox.DiscardedAt, Is.EqualTo(observedAt));
            Assert.That(
                supersededOutbox.DiscardReason,
                Is.EqualTo(OutboxDiscardReason.Superseded));
        });
    }

    [Test]
    public async Task UnauthenticatedAndStaleUsersCannotResendInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-auth-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Auth Organization",
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

        using var anonymousResponse = await anonymousClient.PostAsync(
            ResendPath(organization.OrganizationId, invitation.InvitationId),
            content: null);
        using var staleResponse = await PostResendAsync(
            staleClient,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.Headers.CacheControl?.NoStore, Is.True);
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task MissingAntiforgeryTokenRejectsResendWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-csrf-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend CSRF Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await client.PostAsync(
            ResendPath(organization.OrganizationId, invitation.InvitationId),
            content: null);

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task InvalidUnknownAndHiddenOrganizationsShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-private-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "resend-private-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Private Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);

        using var invalidResponse = await PostResendAsync(
            ownerClient,
            Guid.NewGuid(),
            invitation.InvitationId);
        using var malformedResponse = await PostResendAsync(
            ownerClient,
            TestIdentifiers.MalformedText,
            invitation.InvitationId.ToString());
        using var nonRfcVariantResponse = await PostResendAsync(
            ownerClient,
            TestIdentifiers.Version7WithNonRfcVariant,
            invitation.InvitationId);
        using var unknownResponse = await PostResendAsync(
            ownerClient,
            Guid.CreateVersion7(),
            invitation.InvitationId);
        using var hiddenResponse = await PostResendAsync(
            outsiderClient,
            organization.OrganizationId,
            invitation.InvitationId);
        var responses = new[]
        {
            invalidResponse,
            malformedResponse,
            nonRfcVariantResponse,
            unknownResponse,
            hiddenResponse,
        };
        var reasonCodes = await Task.WhenAll(responses.Select(ReadReasonCodeAsync));

        for (var index = 0; index < responses.Length; index++)
        {
            Assert.Multiple(() =>
            {
                Assert.That(responses[index].StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(responses[index].Headers.CacheControl?.NoStore, Is.True);
                Assert.That(reasonCodes[index],
                    Is.EqualTo(OrganizationInvitationReasonCodes.OrganizationNotFound));
            });
        }

        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task MemberCannotResendInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-member-owner", "owner@example.com");
        var member = await SignUpAsync(database, "resend-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Member Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, member.UserId, OrganizationRole.Member);
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(member.UserId);

        using var response = await PostResendAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(reasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.InsufficientPermission));
        });
    }

    [Test]
    public async Task InvalidUnknownCrossOrganizationAndCancelledInvitationsShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-invite-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Invitation Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Other API Organization",
            SignupTime.AddDays(2));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using (var context = database.CreateContext())
        {
            await using var serviceTest1 = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(database, SignupTime.AddDays(3));
            await serviceTest1.Service
                .CancelAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    invitation.InvitationId);
        }

        var baseline = await CaptureWritesAsync(database);

        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(4));
        using var client = factory.CreateApiClient(owner.UserId);
        using var invalidResponse = await PostResendAsync(
            client,
            organization.OrganizationId,
            Guid.NewGuid());
        using var malformedResponse = await PostResendAsync(
            client,
            organization.OrganizationId.ToString(),
            TestIdentifiers.MalformedText);
        using var nonRfcVariantResponse = await PostResendAsync(
            client,
            organization.OrganizationId,
            TestIdentifiers.Version7WithNonRfcVariant);
        using var unknownResponse = await PostResendAsync(
            client,
            organization.OrganizationId,
            Guid.CreateVersion7());
        using var crossResponse = await PostResendAsync(
            client,
            otherOrganization.OrganizationId,
            invitation.InvitationId);
        using var cancelledResponse = await PostResendAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        var responses = new[]
        {
            invalidResponse,
            malformedResponse,
            nonRfcVariantResponse,
            unknownResponse,
            crossResponse,
            cancelledResponse,
        };
        var reasonCodes = await Task.WhenAll(responses.Select(ReadReasonCodeAsync));

        for (var index = 0; index < responses.Length; index++)
        {
            Assert.Multiple(() =>
            {
                Assert.That(responses[index].StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(reasonCodes[index],
                    Is.EqualTo(OrganizationInvitationReasonCodes.InvitationNotFound));
            });
        }

        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task MemberAndCapacityConflictsExposeSpecificReasonCodes()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-conflict-owner", "owner@example.com");
        var invitee = await SignUpAsync(database, "resend-conflict-invitee", "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Conflict Organization",
            SignupTime.AddDays(1));
        var memberInvitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await AddOrganizationMemberAsync(database, organization, invitee.UserId, OrganizationRole.Member);
        var capacityInvitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "capacity@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(30));
        using var client = factory.CreateApiClient(owner.UserId);

        using var memberResponse = await PostResendAsync(
            client,
            organization.OrganizationId,
            memberInvitation.InvitationId);
        using var capacityResponse = await PostResendAsync(
            client,
            organization.OrganizationId,
            capacityInvitation.InvitationId);
        var memberReasonCode = await ReadReasonCodeAsync(memberResponse);
        var capacityReasonCode = await ReadReasonCodeAsync(capacityResponse);

        Assert.Multiple(() =>
        {
            Assert.That(memberResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(memberReasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.AlreadyMember));
            Assert.That(capacityResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(capacityReasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.NoActiveSeatCapacity));
        });
    }

    [Test]
    public async Task AnotherPendingInvitationReturnsSpecificConflictWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-pending-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Pending API Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "INVITEE@example.com",
            SignupTime.AddDays(10));
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(10));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostResendAsync(
            client,
            organization.OrganizationId,
            expired.InvitationId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                reasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.InvitationAlreadyPending));
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task FullOrganizationReturnsSpecificConflictWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-full-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Full API Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired@example.com",
            SignupTime.AddDays(2));
        for (var index = 0; index < 4; index++)
        {
            await CreateInvitationAsync(
                database,
                owner.UserId,
                organization.OrganizationId,
                $"active-{index}@example.com",
                SignupTime.AddDays(10));
        }

        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(10));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostResendAsync(
            client,
            organization.OrganizationId,
            expired.InvitationId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(reasonCode, Is.EqualTo(OrganizationInvitationReasonCodes.SeatCapacityReached));
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task OlderResendReturnsSupersededConflictWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-superseded-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Superseded API Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using (var setupContext = database.CreateContext())
        {
            await using var serviceTest2 = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(database, SignupTime.AddDays(4));
            var setupResult = await serviceTest2.Service
                .ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    invitation.InvitationId);
            Assert.That(setupResult.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
        }

        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostResendAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(reasonCode, Is.EqualTo(OrganizationInvitationReasonCodes.ResendSuperseded));
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task CancelledRequestRollsBackSecretWindowAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-request-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Request Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        var gate = new DatabaseCommandGate();
        var saveGateInterceptor = new SavedChangesGateInterceptor(gate);
        var path = ResendPath(organization.OrganizationId, invitation.InvitationId);
        var completionObserver = new RequestCompletionObserver(HttpMethods.Post, path);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3),
            useTestAuthentication: true,
            requestCompletionObserver: completionObserver,
            interceptors: [saveGateInterceptor]);
        using var client = factory.CreateApiClient(owner.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = PostResendAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId,
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

    private static Task<HttpResponseMessage> PostResendAsync(
        HttpClient client,
        Guid organizationId,
        Guid invitationId,
        CancellationToken cancellationToken = default)
    {
        return PostResendAsync(
            client,
            organizationId.ToString(),
            invitationId.ToString(),
            cancellationToken);
    }

    private static async Task<HttpResponseMessage> PostResendAsync(
        HttpClient client,
        string organizationId,
        string invitationId,
        CancellationToken cancellationToken = default)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(
            HttpMethod.Post,
            ResendPath(organizationId, invitationId));
        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request, cancellationToken);
    }

    private static string ResendPath(Guid organizationId, Guid invitationId)
    {
        return ResendPath(organizationId.ToString(), invitationId.ToString());
    }

    private static string ResendPath(string organizationId, string invitationId)
    {
        return $"/api/organizations/{organizationId}/invitations/{invitationId}/resend";
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        var content = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(content);
        return body.RootElement.GetProperty("reasonCode").GetString();
    }

    private static async Task<ResendWriteSnapshot> CaptureWritesAsync(
        PostgresTestDatabase database)
    {
        await using var context = database.CreateContext();
        var invitations = await context.OrganizationInvitations
            .AsNoTracking()
            .OrderBy(invitation => invitation.Id)
            .Select(invitation => new InvitationSnapshot(
                invitation.Id,
                invitation.SecretHash,
                invitation.LastSentAt,
                invitation.ExpiresAt))
            .ToArrayAsync();
        var outboxMessages = await context.OutboxMessages
            .AsNoTracking()
            .OrderBy(message => message.Id)
            .Select(message => new OutboxSnapshot(
                message.Id,
                message.CorrelationId,
                message.SubjectId,
                message.OccurredAt,
                message.NotAfter,
                message.NativeOutboxEnqueued,
                message.DiscardedAt,
                message.DiscardReason))
            .ToArrayAsync();
        return new ResendWriteSnapshot(
            invitations,
            await context.AuditRecords.CountAsync(
                record => record.Action == AuditAction.OrganizationInvitationResent),
            outboxMessages);
    }

    private static void AssertWritesUnchanged(
        ResendWriteSnapshot before,
        ResendWriteSnapshot after)
    {
        Assert.Multiple(() =>
        {
            Assert.That(after.Invitations, Is.EqualTo(before.Invitations));
            Assert.That(after.AuditCount, Is.EqualTo(before.AuditCount));
            Assert.That(after.OutboxMessages, Is.EqualTo(before.OutboxMessages));
        });
    }

    private sealed record ResendWriteSnapshot(
        IReadOnlyList<InvitationSnapshot> Invitations,
        int AuditCount,
        IReadOnlyList<OutboxSnapshot> OutboxMessages);

    private sealed record InvitationSnapshot(
        Guid InvitationId,
        string SecretHash,
        DateTimeOffset LastSentAt,
        DateTimeOffset ExpiresAt);

    private sealed record OutboxSnapshot(
        Guid Id,
        Guid CorrelationId,
        Guid SubjectId,
        DateTimeOffset OccurredAt,
        DateTimeOffset? NotAfter,
        bool NativeOutboxEnqueued,
        DateTimeOffset? DiscardedAt,
        OutboxDiscardReason? DiscardReason);
}
