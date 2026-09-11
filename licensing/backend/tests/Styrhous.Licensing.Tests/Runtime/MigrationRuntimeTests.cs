using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Infrastructure.Identity;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
public sealed class MigrationRuntimeTests
{
    [Test]
    public async Task MigrateCommandCreatesTheCompleteSchemaAndRecordsTheBaseline()
    {
        await using var database = await PostgresTestDatabase.CreateUninitializedAsync();

        await Program.RunMigrationsAsync(
            [$"--ConnectionStrings:Licensing={database.ConnectionString}"]);

        await using var context = database.CreateContext();
        var applied = await context.Database.GetAppliedMigrationsAsync();
        var hasPendingModelChanges = context.Database.HasPendingModelChanges();
        var userCount = await context.UserAccounts.CountAsync();
        var identityUserCount = await context.Set<ApplicationIdentityUser>().CountAsync();
        var desktopSessionCount = await context.DesktopDeviceSessions.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(applied, Has.Exactly(1).EndsWith("_InitialLicensingSchema"));
            Assert.That(userCount, Is.Zero);
            Assert.That(identityUserCount, Is.Zero);
            Assert.That(desktopSessionCount, Is.Zero);
            Assert.That(hasPendingModelChanges, Is.False);
        });
    }
}
