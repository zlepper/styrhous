using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Signups;

namespace Styrhous.Licensing.Application.Signups;

public sealed class UserSignupService(
    PostgresUserSignupStore store,
    IDbContextFactory<LicensingDbContext> contextFactory,
    TimeProvider timeProvider)
{

    public async Task<SignupResult> SignUpAsync(
        VerifiedExternalIdentity identity,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(identity);

        if (store.HasActiveTransaction)
        {
            return await SignUpCoreAsync(store, identity, cancellationToken);
        }

        return await EfConcurrencyRetry.ExecuteAsync(() =>
            LicensingDbContextTransaction.ExecuteAsync(
                contextFactory,
                IsolationLevel.Serializable,
                (operationContext, token) => SignUpCoreAsync(
                    new PostgresUserSignupStore(operationContext, contextFactory),
                    identity,
                    token),
                cancellationToken));
    }

    private async Task<SignupResult> SignUpCoreAsync(
        PostgresUserSignupStore operationStore,
        VerifiedExternalIdentity identity,
        CancellationToken cancellationToken)
    {
        var observedAt = timeProvider.GetUtcNow();
        var existing = await FindAndSynchronizeAsync(
            operationStore,
            identity,
            observedAt,
            cancellationToken);
        if (existing is not null)
        {
            return existing;
        }

        if (await operationStore.IsVerifiedEmailClaimedAsync(
                identity.NormalizedEmail,
                cancellationToken))
        {
            throw new AccountLinkRequiredException();
        }

        var registration = SignupRegistration.Start(identity, observedAt);
        try
        {
            await operationStore.AddAsync(registration, cancellationToken);
            return SignupResult.Created(registration);
        }
        catch (DuplicateExternalIdentityException)
        {
            return await FindAndSynchronizeAsync(
                operationStore,
                identity,
                observedAt,
                cancellationToken)
                ?? throw new InvalidOperationException(
                    "The external identity became unavailable after a duplicate signup was detected.");
        }
        catch (AccountLinkRequiredException)
        {
            var concurrentlyCreated = await FindAndSynchronizeAsync(
                operationStore,
                identity,
                observedAt,
                cancellationToken);
            if (concurrentlyCreated is not null)
            {
                return concurrentlyCreated;
            }

            throw;
        }
    }

    private static async Task<SignupResult?> FindAndSynchronizeAsync(
        PostgresUserSignupStore store,
        VerifiedExternalIdentity identity,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        var existing = await store.FindAsync(identity, cancellationToken);
        if (existing is not null)
        {
            await store.SynchronizeVerifiedEmailAsync(
                identity,
                observedAt,
                cancellationToken);
        }

        return existing;
    }
}
