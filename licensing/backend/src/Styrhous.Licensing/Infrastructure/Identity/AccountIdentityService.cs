using System.Data;
using Microsoft.AspNetCore.Identity;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Infrastructure.Identity;

public sealed class AccountIdentityService(
    LicensingDbContext dbContext,
    UserManager<ApplicationIdentityUser> userManager,
    UserSignupService signup,
    ExternalAccountService accounts,
    IServiceScopeFactory scopeFactory)
{

    public Task<ApplicationIdentityUser> SignInOrCreateAsync(
        VerifiedExternalIdentity externalIdentity,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(externalIdentity);
        return ExecuteSerializableAsync(
            service => service.SignInOrCreateCoreAsync(externalIdentity, cancellationToken),
            cancellationToken);
    }

    private async Task<ApplicationIdentityUser> SignInOrCreateCoreAsync(
        VerifiedExternalIdentity externalIdentity,
        CancellationToken cancellationToken)
    {
        var registration = await signup.SignUpAsync(
            externalIdentity,
            cancellationToken);
        var identityUser = await userManager.FindByIdAsync(
            registration.UserId.ToString());
        if (identityUser is null)
        {
            identityUser = ApplicationIdentityUser.Create(
                registration.UserId,
                externalIdentity.VerifiedEmail);
            RequireSuccess(
                await userManager.CreateAsync(identityUser),
                "create the application identity");
        }

        var loginUser = await userManager.FindByLoginAsync(
            externalIdentity.Provider,
            externalIdentity.Subject);
        if (loginUser is not null && loginUser.Id != registration.UserId)
        {
            throw new InvalidOperationException(
                "The framework and domain external-login owners differ.");
        }

        if (loginUser is null)
        {
            RequireSuccess(
                await userManager.AddLoginAsync(
                    identityUser,
                    ToLogin(externalIdentity)),
                "record the external login");
        }

        if (!string.Equals(
                identityUser.Email,
                externalIdentity.VerifiedEmail,
                StringComparison.Ordinal))
        {
            RequireSuccess(
                await userManager.SetEmailAsync(
                    identityUser,
                    externalIdentity.VerifiedEmail),
                "synchronize the verified email");
            identityUser.EmailConfirmed = true;
            RequireSuccess(
                await userManager.UpdateAsync(identityUser),
                "confirm the synchronized verified email");
        }

        return identityUser;
    }

    public Task<ApplicationIdentityUser> LinkAsync(
        Guid userId,
        VerifiedExternalIdentity externalIdentity,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(externalIdentity);
        return ExecuteSerializableAsync(
            service => service.LinkCoreAsync(userId, externalIdentity, cancellationToken),
            cancellationToken);
    }

    private async Task<ApplicationIdentityUser> LinkCoreAsync(
        Guid userId,
        VerifiedExternalIdentity externalIdentity,
        CancellationToken cancellationToken)
    {
        var identityUser = await RequireUserAsync(userId);
        var loginUser = await userManager.FindByLoginAsync(
            externalIdentity.Provider,
            externalIdentity.Subject);
        if (loginUser is not null && loginUser.Id != userId)
        {
            throw new AccountProviderAlreadyLinkedException();
        }

        await accounts.LinkAsync(
            userId,
            externalIdentity,
            cancellationToken);
        if (loginUser is null)
        {
            RequireSuccess(
                await userManager.AddLoginAsync(
                    identityUser,
                    ToLogin(externalIdentity)),
                "record the linked external login");
        }

        return identityUser;
    }

    public async Task<ApplicationIdentityUser?> FindLinkedUserAsync(
        Guid userId,
        VerifiedExternalIdentity externalIdentity,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(externalIdentity);
        cancellationToken.ThrowIfCancellationRequested();
        var loginUser = await userManager.FindByLoginAsync(
            externalIdentity.Provider,
            externalIdentity.Subject);
        return loginUser?.Id == userId
            && await accounts.IsLinkedAsync(
                userId,
                externalIdentity,
                cancellationToken)
                    ? loginUser
                    : null;
    }

    public async Task UnlinkAsync(
        Guid userId,
        string provider,
        CancellationToken cancellationToken = default)
    {
        _ = await ExecuteSerializableAsync(
            service => service.UnlinkCoreAsync(userId, provider, cancellationToken),
            cancellationToken);
    }

    private async Task<bool> UnlinkCoreAsync(
        Guid userId,
        string provider,
        CancellationToken cancellationToken)
    {
        var identityUser = await RequireUserAsync(userId);
        var domainIdentity = await dbContext.ExternalIdentities
            .AsNoTracking()
            .SingleOrDefaultAsync(
                identity => identity.UserId == userId
                    && identity.Provider == provider,
                cancellationToken);
        if (domainIdentity is null)
        {
            throw new AccountProviderNotLinkedException();
        }

        var loginUser = await userManager.FindByLoginAsync(
            provider,
            domainIdentity.Subject);
        if (loginUser is not null && loginUser.Id != userId)
        {
            throw new InvalidOperationException(
                "The framework and domain external-login owners differ.");
        }

        await accounts.UnlinkAsync(userId, provider, cancellationToken);
        if (loginUser is not null)
        {
            RequireSuccess(
                await userManager.RemoveLoginAsync(
                    identityUser,
                    provider,
                    domainIdentity.Subject),
                "remove the external login");
        }

        return true;
    }

    private async Task<ApplicationIdentityUser> RequireUserAsync(Guid userId)
    {
        return await userManager.FindByIdAsync(userId.ToString())
        ?? throw new InvalidOperationException(
            "The authenticated account has no framework identity.");
    }

    private async Task<T> ExecuteSerializableAsync<T>(
        Func<AccountIdentityService, Task<T>> operation,
        CancellationToken cancellationToken)
    {
        if (dbContext.Database.CurrentTransaction is not null)
        {
            // The enclosing operation owns rollback and retry of its entire scope.
            return await operation(this);
        }

        return await EfConcurrencyRetry.ExecuteAsync(async () =>
        {
            await using var scope = scopeFactory.CreateAsyncScope();
            var operationContext = scope.ServiceProvider.GetRequiredService<LicensingDbContext>();
            var service = scope.ServiceProvider.GetRequiredService<AccountIdentityService>();
            await using var transaction = await operationContext.Database.BeginTransactionAsync(
                IsolationLevel.Serializable,
                cancellationToken);
            var result = await operation(service);
            await transaction.CommitAsync(cancellationToken);
            return result;
        });
    }

    private static UserLoginInfo ToLogin(VerifiedExternalIdentity identity)
    {
        return new(identity.Provider, identity.Subject, identity.Provider);
    }

    private static void RequireSuccess(IdentityResult result, string operation)
    {
        if (result.Succeeded)
        {
            return;
        }

        if (result.Errors.Any(error =>
                string.Equals(
                    error.Code,
                    "ConcurrencyFailure",
                    StringComparison.Ordinal)
                || string.Equals(
                    error.Code,
                    "DuplicateUserName",
                    StringComparison.Ordinal)
                || string.Equals(
                    error.Code,
                    "LoginAlreadyAssociated",
                    StringComparison.Ordinal)))
        {
            throw new DbUpdateConcurrencyException(
                $"Identity could not {operation} because another request changed it.");
        }

        throw new InvalidOperationException($"Identity could not {operation}.");
    }
}
