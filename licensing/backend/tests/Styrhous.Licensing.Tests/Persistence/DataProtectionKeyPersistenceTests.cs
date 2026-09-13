using Microsoft.AspNetCore.DataProtection;
using Microsoft.Extensions.DependencyInjection;
using Styrhous.Licensing.Tests.Api;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
public sealed class DataProtectionKeyPersistenceTests
{
    [Test]
    public async Task KeysPersistAcrossServiceProviders()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var firstFactory = new LicensingWebApplicationFactory(
            database,
            LicensingPersistenceScenario.SignupTime);
        var protectedValue = firstFactory.Services
            .GetRequiredService<IDataProtectionProvider>()
            .CreateProtector("persistence-test")
            .Protect("protected invitation delivery");

        using var secondFactory = new LicensingWebApplicationFactory(
            database,
            LicensingPersistenceScenario.SignupTime);
        var recoveredValue = secondFactory.Services
            .GetRequiredService<IDataProtectionProvider>()
            .CreateProtector("persistence-test")
            .Unprotect(protectedValue);
        await using var context = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(recoveredValue, Is.EqualTo("protected invitation delivery"));
            Assert.That(context.DataProtectionKeys, Is.Not.Empty);
        });
    }
}
