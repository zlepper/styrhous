using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationInvitationResendService(
    PostgresOrganizationInvitationResendStore store,
    CryptographicOrganizationInvitationSecretGenerator secretGenerator,
    TimeProvider timeProvider)
{

    public async Task<OrganizationInvitationResendResult> ResendAsync(
        Guid actorUserId,
        Guid organizationId,
        Guid invitationId,
        CancellationToken cancellationToken = default)
    {

        var secret = secretGenerator.Generate();
        var observedAt = timeProvider.GetUtcNow();
        var correlationId = Uuid7.Create();
        var stored = await store.ResendAsync(
            actorUserId,
            organizationId,
            invitationId,
            secret,
            observedAt,
            correlationId,
            cancellationToken);
        return stored switch
        {
            OrganizationInvitationResendStoreResult.Success success =>
                OrganizationInvitationResendResult.Resent(
                invitationId,
                correlationId,
                secret,
                observedAt.Add(OrganizationInvitation.Lifetime),
                    success.BackgroundWork),
            OrganizationInvitationResendStoreResult.Rejection rejection =>
                OrganizationInvitationResendResult.Rejected(rejection.Status),
            _ => throw new InvalidOperationException(
                $"Unsupported invitation resend store result: {stored.GetType().Name}."),
        };
    }
}
