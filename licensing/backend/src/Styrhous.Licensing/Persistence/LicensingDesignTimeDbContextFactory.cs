using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Design;
using OpenIddict.EntityFrameworkCore;

namespace Styrhous.Licensing.Persistence;

internal sealed class LicensingDesignTimeDbContextFactory
    : IDesignTimeDbContextFactory<LicensingDbContext>
{
    public LicensingDbContext CreateDbContext(string[] args)
    {
        var connectionString = Environment.GetEnvironmentVariable(
            "STYRHOUS_LICENSING_MIGRATION_CONNECTION")
            ?? "Host=127.0.0.1;Database=styrhous_licensing;Username=styrhous";
        var builder = new DbContextOptionsBuilder<LicensingDbContext>();
        builder.UseNpgsql(connectionString);
        builder.UseOpenIddict<Guid>();
        return new LicensingDbContext(builder.Options);
    }
}
