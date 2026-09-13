using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

internal static class TestDbContextFactory
{
    public static IDbContextFactory<LicensingDbContext> CreateContextFactory(this LicensingDbContext context)
    {
        // Retain the real database configuration and concurrency interceptors for every attempt.
        var options = (DbContextOptions<LicensingDbContext>)context.GetService<IDbContextOptions>();
        return new Factory(options);
    }

    private sealed class Factory(DbContextOptions<LicensingDbContext> options)
        : IDbContextFactory<LicensingDbContext>
    {
        public LicensingDbContext CreateDbContext()
        {
            return new LicensingDbContext(options);
        }
    }
}
