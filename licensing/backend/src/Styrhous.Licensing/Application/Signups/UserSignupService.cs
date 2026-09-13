using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Signups;

namespace Styrhous.Licensing.Application.Signups;

public sealed class UserSignupService(PostgresUserSignupStore store, TimeProvider timeProvider)
{

    public async Task<SignupResult> SignUpAsync(
        VerifiedExternalIdentity identity,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(identity);

        var observedAt = timeProvider.GetUtcNow();
        var existing = await FindAndSynchronizeAsync(identity, observedAt, cancellationToken);
        if (existing is not null)
        {
            return existing;
        }

        if (await store.IsVerifiedEmailClaimedAsync(identity.NormalizedEmail, cancellationToken))
        {
            throw new AccountLinkRequiredException();
        }

        var registration = SignupRegistration.Start(identity, observedAt);
        try
        {
            await store.AddAsync(registration, cancellationToken);
            return SignupResult.Created(registration);
        }
        catch (DuplicateExternalIdentityException)
        {
            return await FindAndSynchronizeAsync(identity, observedAt, cancellationToken)
                ?? throw new InvalidOperationException(
                    "The external identity became unavailable after a duplicate signup was detected.");
        }
        catch (AccountLinkRequiredException)
        {
            var concurrentlyCreated = await FindAndSynchronizeAsync(
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

    private async Task<SignupResult?> FindAndSynchronizeAsync(
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
