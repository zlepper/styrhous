using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class ExternalAccountServiceTests
{
    private static readonly string[] GitHubAndGoogle = ["github", "google"];
    private static readonly string[] GitHubOnly = ["github"];

    [Test]
    public async Task MatchingVerifiedEmailCanLinkAndUnlinkASecondProvider()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "github-user", "person@example.com");
        await using var test = ServiceTestBase<ExternalAccountService>.ForDatabase(database, SignupTime.AddHours(1));
        var service = test.Service;

        await service.LinkAsync(
            signup.UserId,
            VerifiedExternalIdentity.Create("google", "google-user", "PERSON@example.com"));
        var linked = await service.ListProvidersAsync(signup.UserId);
        await service.UnlinkAsync(signup.UserId, "google");
        var remaining = await service.ListProvidersAsync(signup.UserId);

        Assert.Multiple(() =>
        {
            Assert.That(linked, Is.EqualTo(GitHubAndGoogle));
            Assert.That(remaining, Is.EqualTo(GitHubOnly));
        });
    }

    [Test]
    public async Task LinkNeverMergesAccountsFromAProviderEmailAlone()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var first = await SignUpAsync(database, "first-user", "first@example.com");
        var second = await SignUpAsync(database, "second-user", "second@example.com");
        await using var test = ServiceTestBase<ExternalAccountService>.ForDatabase(database, SignupTime.AddHours(1));
        var service = test.Service;

        Assert.That(
            async () => await service.LinkAsync(
                first.UserId,
                VerifiedExternalIdentity.Create(
                    "google",
                    "second-provider-subject",
                    "second@example.com")),
            Throws.TypeOf<AccountLinkEmailMismatchException>());
        Assert.That(await service.ListProvidersAsync(second.UserId), Is.EqualTo(GitHubOnly));
    }

    [Test]
    public async Task LastProviderCannotBeRemoved()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "only-user", "only@example.com");
        await using var test = ServiceTestBase<ExternalAccountService>.ForDatabase(database, SignupTime.AddHours(1));
        var service = test.Service;

        Assert.That(
            async () => await service.UnlinkAsync(signup.UserId, "github"),
            Throws.TypeOf<LastAccountProviderException>());
    }

    [Test]
    public async Task ConcurrentUnlinksCannotRemoveTheLastProvider()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database, "concurrent-user", "person@example.com");
        await using (var setup = ServiceTestBase<ExternalAccountService>.ForDatabase(database, SignupTime.AddHours(1)))
        {
            await setup.Service
                .LinkAsync(
                    signup.UserId,
                    VerifiedExternalIdentity.Create(
                        "google",
                        "concurrent-google-user",
                        "person@example.com"));
        }

        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        async Task<Exception?> UnlinkAsync(string provider)
        {
            await using var test = ServiceTestBase<ExternalAccountService>.ForDatabase(
                database, SignupTime.AddHours(2), interceptors: [new DatabaseCommandBarrierInterceptor(
                    barrier, "FROM external_identities", DatabaseCommandInterceptionPhase.AfterReaderExecution)]);
            var service = test.Service;
            try
            {
                await service.UnlinkAsync(signup.UserId, provider);
                return null;
            }
            catch (Exception exception)
            {
                return exception;
            }
        }

        var outcomes = await Task.WhenAll(
            UnlinkAsync("github"),
            UnlinkAsync("google"));
        await using var verification = ServiceTestBase<ExternalAccountService>.ForDatabase(database, SignupTime.AddHours(2));
        var remaining = await verification.Service
            .ListProvidersAsync(signup.UserId);

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(outcomes, Has.Exactly(1).Null);
            Assert.That(outcomes, Has.Exactly(1).TypeOf<LastAccountProviderException>());
            Assert.That(remaining, Has.Count.EqualTo(1));
        });
    }
}
