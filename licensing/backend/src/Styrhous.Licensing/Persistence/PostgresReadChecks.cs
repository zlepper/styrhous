using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresReadChecks
{
    public static async Task EnsureUserExistsAsync(
        LicensingDbContext dbContext,
        Guid userId,
        CancellationToken cancellationToken)
    {
        var userExists = await dbContext.UserAccounts
            .AsNoTracking()
            .AnyAsync(user => user.Id == userId, cancellationToken);
        if (!userExists)
        {
            throw new UserNotFoundException();
        }
    }
}
