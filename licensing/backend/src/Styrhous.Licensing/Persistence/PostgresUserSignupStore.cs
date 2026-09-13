using Microsoft.EntityFrameworkCore;
using Npgsql;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Signups;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresUserSignupStore(
    LicensingDbContext dbContext,
    IDbContextFactory<LicensingDbContext> contextFactory)
{
    internal bool HasActiveTransaction => dbContext.Database.CurrentTransaction is not null;

    public Task<SignupResult?> FindAsync(
        VerifiedExternalIdentity identity,
        CancellationToken cancellationToken)
    {
        return (
            from externalIdentity in dbContext.ExternalIdentities.AsNoTracking()
            join user in dbContext.UserAccounts.AsNoTracking()
                on externalIdentity.UserId equals user.Id
            join billingAccount in dbContext.BillingAccounts.AsNoTracking()
                on (Guid?)user.Id equals billingAccount.PersonalOwnerUserId
            join seat in dbContext.Seats.AsNoTracking()
                on billingAccount.Id equals seat.BillingAccountId
            join trial in dbContext.Trials.AsNoTracking()
                on user.Id equals trial.OriginatingUserId
            where externalIdentity.Provider == identity.Provider
                && externalIdentity.Subject == identity.Subject
                && billingAccount.Kind == BillingAccountKind.Personal
                && seat.AssignedUserId == user.Id
            select new SignupResult(
                user.Id,
                billingAccount.Id,
                seat.Id,
                trial.Id,
                trial.StartedAt,
                trial.EndsAt,
                false)
        ).SingleOrDefaultAsync(cancellationToken);
    }

    public Task<bool> IsVerifiedEmailClaimedAsync(
        string normalizedEmail,
        CancellationToken cancellationToken)
    {
        return dbContext.VerifiedEmailClaims
            .AsNoTracking()
            .AnyAsync(claim => claim.NormalizedEmail == normalizedEmail, cancellationToken);
    }

    public async Task SynchronizeVerifiedEmailAsync(
        VerifiedExternalIdentity identity,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        if (dbContext.Database.CurrentTransaction is not null)
        {
            await TrySynchronizeVerifiedEmailAsync(dbContext, identity, observedAt, cancellationToken);
            return;
        }

        await EfConcurrencyRetry.ExecuteAsync(async () =>
        {
            await using var operationContext = await contextFactory.CreateDbContextAsync(cancellationToken);
            await TrySynchronizeVerifiedEmailAsync(operationContext, identity, observedAt, cancellationToken);
            return true;
        });
    }

    private static async Task TrySynchronizeVerifiedEmailAsync(
        LicensingDbContext dbContext,
        VerifiedExternalIdentity identity,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        var user = await (
            from externalIdentity in dbContext.ExternalIdentities
            join candidate in dbContext.UserAccounts on externalIdentity.UserId equals candidate.Id
            where externalIdentity.Provider == identity.Provider
                && externalIdentity.Subject == identity.Subject
            select candidate
        ).SingleAsync(cancellationToken);

        if (user.NormalizedEmail == identity.NormalizedEmail)
        {
            return;
        }

        var claimedUserId = await FindClaimedUserIdAsync(
            dbContext,
            identity.NormalizedEmail,
            cancellationToken);
        if (claimedUserId is not null && claimedUserId != user.Id)
        {
            throw new AccountLinkRequiredException();
        }

        if (claimedUserId is null)
        {
            dbContext.VerifiedEmailClaims.Add(
                VerifiedEmailClaim.Create(user.Id, identity, observedAt));
        }

        user.UpdateVerifiedEmail(identity);
        try
        {
            await dbContext.SaveChangesAsync(cancellationToken);
        }
        catch (DbUpdateException exception) when (IsVerifiedEmailConflict(exception))
        {
            // Recheck ownership only after the complete operation starts with fresh state.
            throw new DbUpdateConcurrencyException(
                "A concurrent operation claimed the verified email.", exception);
        }
    }

    public async Task AddAsync(
        SignupRegistration registration,
        CancellationToken cancellationToken)
    {
        if (dbContext.Database.CurrentTransaction is not null)
        {
            await AddCoreAsync(dbContext, registration, cancellationToken);
            return;
        }

        await using var operationContext = await contextFactory.CreateDbContextAsync(cancellationToken);
        await AddCoreAsync(operationContext, registration, cancellationToken);
    }

    private static async Task AddCoreAsync(
        LicensingDbContext dbContext,
        SignupRegistration registration,
        CancellationToken cancellationToken)
    {
        dbContext.AddRange(
            registration.User,
            registration.ExternalIdentity,
            registration.VerifiedEmailClaim,
            registration.BillingAccount,
            registration.Seat,
            registration.Trial);

        try
        {
            await dbContext.SaveChangesAsync(cancellationToken);
        }
        catch (DbUpdateException exception)
            when (exception.IsUniqueViolation(DatabaseConstraintNames.ExternalIdentityProviderSubject))
        {
            if (dbContext.Database.CurrentTransaction is not null)
            {
                throw new DbUpdateConcurrencyException(
                    "A concurrent transaction registered the external identity.",
                    exception);
            }

            throw new DuplicateExternalIdentityException(
                "The external identity is already registered.",
                exception);
        }
        catch (DbUpdateException exception)
            when (IsVerifiedEmailConflict(exception))
        {
            if (dbContext.Database.CurrentTransaction is not null)
            {
                throw new DbUpdateConcurrencyException(
                    "A concurrent transaction claimed the verified email.",
                    exception);
            }

            throw new AccountLinkRequiredException(exception);
        }
    }

    private static Task<Guid?> FindClaimedUserIdAsync(
        LicensingDbContext dbContext,
        string normalizedEmail,
        CancellationToken cancellationToken)
    {
        return dbContext.VerifiedEmailClaims
            .AsNoTracking()
            .Where(claim => claim.NormalizedEmail == normalizedEmail)
            .Select(claim => (Guid?)claim.UserId)
            .SingleOrDefaultAsync(cancellationToken);
    }

    private static bool IsVerifiedEmailConflict(DbUpdateException exception)
    {
        return exception.IsUniqueViolation(DatabaseConstraintNames.VerifiedEmailClaimNormalizedEmail);
    }
}
