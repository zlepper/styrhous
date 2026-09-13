using Microsoft.EntityFrameworkCore;
using OpenIddict.Abstractions;
using OpenIddict.EntityFrameworkCore.Models;

namespace Styrhous.Licensing.Persistence;

internal static class DesktopSessionRevocation
{
    // These mutations deliberately use the caller's LicensingDbContext. Activation, membership,
    // and recovery operations already own that context and transaction, so EF keeps the domain
    // and OpenIddict rows in one explicit atomic unit without a second persistence lifecycle.
    public static async Task RevokeDesktopAuthorizationsAsync(
        LicensingDbContext dbContext,
        IReadOnlyCollection<Guid> authorizationIds,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(dbContext);
        ArgumentNullException.ThrowIfNull(authorizationIds);
        if (authorizationIds.Count is 0)
        {
            return;
        }

        var sessions = await dbContext.DesktopDeviceSessions
            .Where(session => authorizationIds.Contains(session.AuthorizationId))
            .ToArrayAsync(cancellationToken);
        foreach (var session in sessions)
        {
            session.Revoke(observedAt);
        }

        await RevokeAuthorizationsAsync(
            dbContext,
            authorizationIds,
            cancellationToken);
    }

    public static async Task RevokeForActivationsAsync(
        LicensingDbContext dbContext,
        IReadOnlyCollection<Guid> activationIds,
        DateTimeOffset observedAt,
        bool retainSessionRecords,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(dbContext);
        ArgumentNullException.ThrowIfNull(activationIds);
        if (activationIds.Count is 0)
        {
            return;
        }

        var sessionsQuery = dbContext.DesktopDeviceSessions
            .Where(session => activationIds.Contains(session.ActivationId));
        var sessions = retainSessionRecords
            ? await sessionsQuery.ToArrayAsync(cancellationToken)
            : await sessionsQuery.AsNoTracking().ToArrayAsync(cancellationToken);
        var authorizationIds = sessions
            .Select(session => session.AuthorizationId)
            .ToArray();
        if (retainSessionRecords)
        {
            foreach (var session in sessions)
            {
                session.Revoke(observedAt);
            }
        }

        await RevokeAuthorizationsAsync(
            dbContext,
            authorizationIds,
            cancellationToken);
    }

    public static async Task RevokeAuthorizationsAsync(
        LicensingDbContext dbContext,
        IReadOnlyCollection<Guid> authorizationIds,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(dbContext);
        ArgumentNullException.ThrowIfNull(authorizationIds);
        if (authorizationIds.Count is 0)
        {
            return;
        }

        var authorizationConcurrencyToken = Guid.CreateVersion7().ToString();
        await dbContext
            .Set<OpenIddictEntityFrameworkCoreAuthorization<Guid>>()
            .Where(authorization => authorizationIds.Contains(authorization.Id))
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(
                        authorization => authorization.Status,
                        OpenIddictConstants.Statuses.Revoked)
                    .SetProperty(
                        authorization => authorization.ConcurrencyToken,
                        authorizationConcurrencyToken),
                cancellationToken);

        var tokenConcurrencyToken = Guid.CreateVersion7().ToString();
        await dbContext
            .Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .Where(token => token.Authorization != null
                && authorizationIds.Contains(token.Authorization.Id))
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(
                        token => token.Status,
                        OpenIddictConstants.Statuses.Revoked)
                    .SetProperty(
                        token => token.ConcurrencyToken,
                        tokenConcurrencyToken),
                cancellationToken);
    }
}
