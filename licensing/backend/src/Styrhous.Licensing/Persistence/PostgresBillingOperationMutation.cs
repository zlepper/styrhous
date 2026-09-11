using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresBillingOperationMutation
{
    private const int MaximumMutationAttempts = 3;

    public static Task<bool> WithBillingAccountLockAsync(
        IDbContextFactory<LicensingDbContext> contextFactory,
        Guid operationId,
        Action<BillingOperation> mutation,
        CancellationToken cancellationToken)
    {
        return WithBillingAccountLockAsync(
            contextFactory,
            operationId,
            (_, operation, _) =>
            {
                mutation(operation);
                return Task.FromResult(true);
            },
            missingResult: false,
            cancellationToken);
    }

    public static Task<bool> WithBillingAccountLockAsync(
        IDbContextFactory<LicensingDbContext> contextFactory,
        Guid operationId,
        Func<LicensingDbContext, BillingOperation, CancellationToken, Task> mutation,
        CancellationToken cancellationToken)
    {
        return WithBillingAccountLockAsync(
            contextFactory,
            operationId,
            async (operationContext, operation, token) =>
            {
                await mutation(operationContext, operation, token);
                return true;
            },
            missingResult: false,
            cancellationToken);
    }

    public static async Task<TResult> WithBillingAccountLockAsync<TResult>(
        IDbContextFactory<LicensingDbContext> contextFactory,
        Guid operationId,
        Func<LicensingDbContext, BillingOperation, CancellationToken, Task<TResult>> mutation,
        TResult missingResult,
        CancellationToken cancellationToken)
    {
        for (var attempt = 0; attempt < MaximumMutationAttempts; attempt++)
        {
            await using var dbContext = await contextFactory.CreateDbContextAsync(cancellationToken);
            try
            {
                return await LicensingDbContextTransaction.ExecuteAsync<TResult>(
                    dbContext,
                    async (transaction, token) =>
                    {
                        var billingAccountId = await dbContext.BillingOperations
                    .AsNoTracking()
                    .Where(operation => operation.Id == operationId)
                    .Select(operation => (Guid?)operation.BillingAccountId)
                    .SingleOrDefaultAsync(token);
                        if (billingAccountId is null
                            || !await EfTransactionSerialization.TryClaimBillingAccountAsync(
                                dbContext,
                                billingAccountId.Value,
                                token))
                        {
                            return missingResult;
                        }

                        var operation = await dbContext.BillingOperations.SingleOrDefaultAsync(
                            candidate => candidate.Id == operationId,
                            token);
                        if (operation is null)
                        {
                            return missingResult;
                        }

                        var result = await mutation(dbContext, operation, token);
                        await dbContext.SaveChangesAsync(token);
                        await transaction.CommitAsync(token);
                        return result;
                    },
                    cancellationToken);
            }
            catch (Exception exception) when (
                attempt + 1 < MaximumMutationAttempts
                && EfConcurrencyFailure.IsRetryable(exception))
            {
            }
        }

        return missingResult;
    }
}
