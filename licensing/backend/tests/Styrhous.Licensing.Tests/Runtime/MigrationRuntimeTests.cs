using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
[NonParallelizable]
public sealed class MigrationRuntimeTests
{
    [Test]
    public async Task MigrateCommandCreatesTheSingleCurrentSchemaWithDomainAccountTables()
    {
        await using var database = await PostgresTestDatabase.CreateForMigrationAsync();

        await Program.RunMigrationsAsync([
            $"--ConnectionStrings:Licensing={database.ConnectionString}",
        ]);

        await using var context = database.CreateContext();
        var migrations = context.Database.GetMigrations().ToArray();
        var appliedMigrations = (await context.Database.GetAppliedMigrationsAsync()).ToArray();
        var modelTableNames = context.Model.GetEntityTypes()
            .Select(entity => entity.GetTableName())
            .OfType<string>()
            .Distinct(StringComparer.Ordinal)
            .ToArray();
        var migratedTableNames = await database.GetPublicTableNamesAsync();

        Assert.Multiple(() =>
        {
            Assert.That(migrations, Has.Length.EqualTo(1));
            Assert.That(migrations[0], Does.EndWith("_InitialLicensingSchema"));
            Assert.That(appliedMigrations, Is.EqualTo(migrations));
            Assert.That(
                migratedTableNames,
                Is.SupersetOf(modelTableNames));
            Assert.That(
                migratedTableNames,
                Is.SupersetOf([
                    "user_accounts",
                    "external_identities",
                    "DataProtectionKeys",
                    "OpenIddictTokens",
                ]));
            Assert.That(
                migratedTableNames.Where(table => table.StartsWith("AspNet", StringComparison.Ordinal)),
                Is.Empty);
        });
    }
}
