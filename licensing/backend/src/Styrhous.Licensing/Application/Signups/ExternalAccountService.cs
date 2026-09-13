using System.Data;
using Microsoft.EntityFrameworkCore;
using Npgsql;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Application.Signups;

public sealed class ExternalAccountService(
    LicensingDbContext dbContext,
    IDbContextFactory<LicensingDbContext> contextFactory,
    TimeProvider timeProvider)
{

    public async Task<IReadOnlyList<string>> ListProvidersAsync(
        Guid userId,
        CancellationToken cancellationToken = default)
    {
        return await dbContext.ExternalIdentities
            .AsNoTracking()
            .Where(identity => identity.UserId == userId)
            .OrderBy(identity => identity.Provider)
            .Select(identity => identity.Provider)
            .ToArrayAsync(cancellationToken);
    }

    public async Task<bool> IsLinkedAsync(
        Guid userId,
        VerifiedExternalIdentity identity,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(identity);
        return await dbContext.ExternalIdentities
            .AsNoTracking()
            .AnyAsync(
                candidate => candidate.UserId == userId
                    && candidate.Provider == identity.Provider
                    && candidate.Subject == identity.Subject,
                cancellationToken);
    }

    public async Task LinkAsync(
        Guid userId,
        VerifiedExternalIdentity identity,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(identity);
        if (dbContext.Database.CurrentTransaction is not null)
        {
            await LinkCoreAsync(dbContext, userId, identity, cancellationToken);
            return;
        }

        await using var operationContext = await contextFactory.CreateDbContextAsync(cancellationToken);
        await LinkCoreAsync(operationContext, userId, identity, cancellationToken);
    }

    private async Task LinkCoreAsync(
        LicensingDbContext dbContext,
        Guid userId,
        VerifiedExternalIdentity identity,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(identity);
        var ownsVerifiedEmail = await dbContext.VerifiedEmailClaims
            .AsNoTracking()
            .AnyAsync(
                claim => claim.UserId == userId
                    && claim.NormalizedEmail == identity.NormalizedEmail,
                cancellationToken);
        if (!ownsVerifiedEmail)
        {
            throw new AccountLinkEmailMismatchException();
        }

        var alreadyLinked = await dbContext.ExternalIdentities
            .AsNoTracking()
            .AnyAsync(
                candidate => candidate.UserId == userId
                    && candidate.Provider == identity.Provider,
                cancellationToken);
        if (alreadyLinked)
        {
            throw new AccountProviderAlreadyLinkedException();
        }

        dbContext.ExternalIdentities.Add(ExternalIdentity.Create(
            userId,
            identity,
            timeProvider.GetUtcNow()));
        try
        {
            await dbContext.SaveChangesAsync(cancellationToken);
        }
        catch (DbUpdateException exception)
            when (exception.IsUniqueViolation())
        {
            throw new AccountProviderAlreadyLinkedException(exception);
        }
    }

    public async Task UnlinkAsync(
        Guid userId,
        string provider,
        CancellationToken cancellationToken = default)
    {
        if (dbContext.Database.CurrentTransaction is not null)
        {
            await RemoveIdentityAsync(dbContext, userId, provider, cancellationToken);
            return;
        }

        await EfConcurrencyRetry.ExecuteAsync(async () =>
        {
            await using var operationContext = await contextFactory.CreateDbContextAsync(cancellationToken);
            await using var transaction = await operationContext.Database.BeginTransactionAsync(
                IsolationLevel.Serializable,
                cancellationToken);
            await RemoveIdentityAsync(operationContext, userId, provider, cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return true;
        });
    }

    private static async Task RemoveIdentityAsync(
        LicensingDbContext dbContext,
        Guid userId,
        string provider,
        CancellationToken cancellationToken)
    {
        var identities = await dbContext.ExternalIdentities
            .Where(identity => identity.UserId == userId)
            .ToArrayAsync(cancellationToken);
        var identity = identities.SingleOrDefault(candidate =>
            string.Equals(candidate.Provider, provider, StringComparison.Ordinal));
        if (identity is null)
        {
            throw new AccountProviderNotLinkedException();
        }

        if (identities.Length == 1)
        {
            throw new LastAccountProviderException();
        }

        dbContext.ExternalIdentities.Remove(identity);
        await dbContext.SaveChangesAsync(cancellationToken);
    }
}

public sealed class AccountLinkEmailMismatchException : Exception
{
}

public sealed class AccountProviderAlreadyLinkedException : Exception
{
    public AccountProviderAlreadyLinkedException()
    {
    }

    public AccountProviderAlreadyLinkedException(Exception innerException)
        : base("The external provider is already linked.", innerException)
    {
    }
}

public sealed class AccountProviderNotLinkedException : Exception
{
}

public sealed class LastAccountProviderException : Exception
{
}
