using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public sealed class OrganizationInvitationCreationService(
    PostgresOrganizationInvitationCreationStore store,
    CryptographicOrganizationInvitationSecretGenerator secretGenerator,
    TimeProvider timeProvider)
{

    public async Task<OrganizationInvitationCreationResult> CreateAsync(
        Guid actorUserId,
        Guid organizationId,
        string email,
        OrganizationRole role,
        bool assignProductSeat,
        CancellationToken cancellationToken = default)
    {

        var generatedSecret = secretGenerator.Generate();
        var invitation = OrganizationInvitation.Create(
            organizationId,
            actorUserId,
            email,
            role,
            generatedSecret.Hash,
            timeProvider.GetUtcNow(),
            assignProductSeat);
        var correlationId = Uuid7.Create();
        var stored = await store.CreateAsync(
            invitation,
            generatedSecret,
            correlationId,
            cancellationToken);
        return stored switch
        {
            OrganizationInvitationCreationStoreResult.Success success =>
                OrganizationInvitationCreationResult.Created(
                invitation,
                correlationId,
                generatedSecret,
                    success.BackgroundWork),
            OrganizationInvitationCreationStoreResult.Rejection rejection =>
                OrganizationInvitationCreationResult.Rejected(rejection.Status),
            _ => throw new InvalidOperationException(
                $"Unsupported invitation creation store result: {stored.GetType().Name}."),
        };
    }

    public Task<OrganizationInvitationCreationResult> CreateAsync(
        Guid actorUserId,
        Guid organizationId,
        string email,
        OrganizationRole role,
        CancellationToken cancellationToken = default)
    {
        return CreateAsync(
            actorUserId,
            organizationId,
            email,
            role,
            assignProductSeat: true,
            cancellationToken);
    }
}
