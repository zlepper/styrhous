using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationCancellationApiTests
{
    private static readonly string[] ResponseProperties =
    [
        "reasonCode",
        "invitationId",
        "correlationId",
        "cancelledAt",
    ];

    [Test]
    public async Task OwnerCancelsInvitationThroughInternalApi()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-api-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation API Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var observedAt = SignupTime.AddDays(3);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await DeleteInvitationAsync(
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
                Is.EqualTo(OrganizationInvitationReasonCodes.Cancelled));
            Assert.That(body.RootElement.GetProperty("invitationId").GetGuid(),
                Is.EqualTo(invitation.InvitationId));
            Assert.That(correlationId.Version, Is.EqualTo(7));
            Assert.That(body.RootElement.GetProperty("cancelledAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt));
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ResponseProperties));
            Assert.That(bodyText, Does.Not.Contain("secret").IgnoreCase);
        });
        await using var context = database.CreateContext();
        var persisted = await context.OrganizationInvitations.SingleAsync();
        var auditCount = await context.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationCancelled
                && record.CorrelationId == correlationId);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.CancelledAt, Is.EqualTo(observedAt));
            Assert.That(auditCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task UnauthenticatedAndStaleUsersCannotCancelInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-auth-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Auth Organization",
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

        using var anonymousResponse = await anonymousClient.DeleteAsync(
            InvitationPath(organization.OrganizationId, invitation.InvitationId));
        using var staleResponse = await DeleteInvitationAsync(
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
    public async Task MissingAntiforgeryTokenRejectsCancellationWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-csrf-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation CSRF Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await client.DeleteAsync(
            InvitationPath(organization.OrganizationId, invitation.InvitationId));

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    [Test]
    public async Task InvalidUnknownAndHiddenOrganizationsShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-private-owner", "owner@example.com");
        var outsider = await SignUpAsync(database, "cancel-private-outsider", "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Private Organization",
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

        using var invalidResponse = await DeleteInvitationAsync(
            ownerClient,
            Guid.NewGuid(),
            invitation.InvitationId);
        using var malformedResponse = await DeleteInvitationAsync(
            ownerClient,
            TestIdentifiers.MalformedText,
            invitation.InvitationId.ToString());
        using var nonRfcVariantResponse = await DeleteInvitationAsync(
            ownerClient,
            TestIdentifiers.Version7WithNonRfcVariant,
            invitation.InvitationId);
        using var unknownResponse = await DeleteInvitationAsync(
            ownerClient,
            Guid.CreateVersion7(),
            invitation.InvitationId);
        using var hiddenResponse = await DeleteInvitationAsync(
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
    public async Task MemberCannotCancelInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-member-owner", "owner@example.com");
        var member = await SignUpAsync(database, "cancel-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Member Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(member.UserId);

        using var response = await DeleteInvitationAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(reasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.InsufficientPermission));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
    }

    [Test]
    public async Task InvalidUnknownCrossOrganizationAndRepeatedInvitationsShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-invite-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Invitation Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Other API Organization",
            SignupTime.AddDays(2));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(owner.UserId);

        using var invalidResponse = await DeleteInvitationAsync(
            client,
            organization.OrganizationId,
            Guid.NewGuid());
        using var malformedResponse = await DeleteInvitationAsync(
            client,
            organization.OrganizationId.ToString(),
            TestIdentifiers.MalformedText);
        using var nonRfcVariantResponse = await DeleteInvitationAsync(
            client,
            organization.OrganizationId,
            TestIdentifiers.Version7WithNonRfcVariant);
        using var unknownResponse = await DeleteInvitationAsync(
            client,
            organization.OrganizationId,
            Guid.CreateVersion7());
        using var crossResponse = await DeleteInvitationAsync(
            client,
            otherOrganization.OrganizationId,
            invitation.InvitationId);
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));

        using var cancelledResponse = await DeleteInvitationAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        using var repeatedResponse = await DeleteInvitationAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        var notFoundResponses = new[]
        {
            invalidResponse,
            malformedResponse,
            nonRfcVariantResponse,
            unknownResponse,
            crossResponse,
            repeatedResponse,
        };
        var reasonCodes = await Task.WhenAll(notFoundResponses.Select(ReadReasonCodeAsync));

        Assert.That(cancelledResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
        for (var index = 0; index < notFoundResponses.Length; index++)
        {
            Assert.Multiple(() =>
            {
                Assert.That(notFoundResponses[index].StatusCode,
                    Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(notFoundResponses[index].Headers.CacheControl?.NoStore, Is.True);
                Assert.That(reasonCodes[index],
                    Is.EqualTo(OrganizationInvitationReasonCodes.InvitationNotFound));
            });
        }
    }

    [Test]
    public async Task CancelledRequestRollsBackInvitationAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-request-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Request Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var baseline = await CaptureWritesAsync(database);
        var gate = new DatabaseCommandGate();
        var saveGateInterceptor = new SavedChangesGateInterceptor(gate);
        var path = InvitationPath(organization.OrganizationId, invitation.InvitationId);
        var completionObserver = new RequestCompletionObserver(HttpMethods.Delete, path);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3),
            useTestAuthentication: true,
            requestCompletionObserver: completionObserver,
            interceptors: [saveGateInterceptor]);
        using var client = factory.CreateApiClient(owner.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = DeleteInvitationAsync(
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

    [Test]
    public async Task OlderCancellationReturnsSupersededConflictWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-superseded-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Superseded API Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using (var setupContext = database.CreateContext())
        {
            await using var serviceTest1 = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(database, SignupTime.AddDays(4));
            var setupResult = await serviceTest1.Service
                .ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    invitation.InvitationId);
            Assert.That(setupResult.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
        }

        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await DeleteInvitationAsync(
            client,
            organization.OrganizationId,
            invitation.InvitationId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                reasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.CancellationSuperseded));
        });
        AssertWritesUnchanged(baseline, await CaptureWritesAsync(database));
    }

    private static Task<HttpResponseMessage> DeleteInvitationAsync(
        HttpClient client,
        Guid organizationId,
        Guid invitationId,
        CancellationToken cancellationToken = default)
    {
        return DeleteInvitationAsync(
            client,
            organizationId.ToString(),
            invitationId.ToString(),
            cancellationToken);
    }

    private static async Task<HttpResponseMessage> DeleteInvitationAsync(
        HttpClient client,
        string organizationId,
        string invitationId,
        CancellationToken cancellationToken = default)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(
            HttpMethod.Delete,
            InvitationPath(organizationId, invitationId));
        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request, cancellationToken);
    }

    private static string InvitationPath(Guid organizationId, Guid invitationId)
    {
        return InvitationPath(organizationId.ToString(), invitationId.ToString());
    }

    private static string InvitationPath(string organizationId, string invitationId)
    {
        return $"/api/organizations/{organizationId}/invitations/{invitationId}";
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        var content = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(content);
        return body.RootElement.GetProperty("reasonCode").GetString();
    }

    private static async Task<CancellationWriteSnapshot> CaptureWritesAsync(
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
                invitation.ExpiresAt,
                invitation.CancelledAt))
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
        return new CancellationWriteSnapshot(
            invitations,
            outboxMessages,
            await context.AuditRecords.CountAsync(
                record => record.Action == AuditAction.OrganizationInvitationCancelled));
    }

    private static void AssertWritesUnchanged(
        CancellationWriteSnapshot before,
        CancellationWriteSnapshot after)
    {
        Assert.Multiple(() =>
        {
            Assert.That(after.Invitations, Is.EqualTo(before.Invitations));
            Assert.That(after.OutboxMessages, Is.EqualTo(before.OutboxMessages));
            Assert.That(after.AuditCount, Is.EqualTo(before.AuditCount));
        });
    }

    private sealed record CancellationWriteSnapshot(
        IReadOnlyList<InvitationSnapshot> Invitations,
        IReadOnlyList<OutboxMessageSnapshot> OutboxMessages,
        int AuditCount);

    private sealed record InvitationSnapshot(
        Guid InvitationId,
        string SecretHash,
        DateTimeOffset LastSentAt,
        DateTimeOffset ExpiresAt,
        DateTimeOffset? CancelledAt);

    private sealed record OutboxMessageSnapshot(
        Guid MessageId,
        Guid SubjectId,
        bool NativeOutboxEnqueued,
        DateTimeOffset? DiscardedAt,
        OutboxDiscardReason? DiscardReason);
}
