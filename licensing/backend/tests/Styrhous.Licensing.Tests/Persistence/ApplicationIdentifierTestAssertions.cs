using Styrhous.Licensing.Application.Accounts;

namespace Styrhous.Licensing.Tests.Persistence;

internal static class ApplicationIdentifierTestAssertions
{
    public static async Task AssertSingleUsesDatabaseAsync(
        Func<Guid, Task> operation,
        DatabaseCommandCounterInterceptor counter)
    {
        foreach (var identifier in TestIdentifiers.ExistingIdentifierFormats)
        {
            await AssertUsesDatabaseAsync(() => operation(identifier), counter);
        }
    }

    public static async Task AssertPairUsesDatabaseAsync(
        Func<Guid, Guid, Task> operation,
        DatabaseCommandCounterInterceptor counter)
    {
        var otherIdentifier = Guid.CreateVersion7();
        foreach (var identifier in TestIdentifiers.ExistingIdentifierFormats)
        {
            await AssertUsesDatabaseAsync(() => operation(identifier, otherIdentifier), counter);
            await AssertUsesDatabaseAsync(() => operation(otherIdentifier, identifier), counter);
        }
    }

    public static async Task AssertTripleUsesDatabaseAsync(
        Func<Guid, Guid, Guid, Task> operation,
        DatabaseCommandCounterInterceptor counter)
    {
        var otherIdentifier = Guid.CreateVersion7();
        foreach (var identifier in TestIdentifiers.ExistingIdentifierFormats)
        {
            await AssertUsesDatabaseAsync(() => operation(identifier, otherIdentifier, otherIdentifier), counter);
            await AssertUsesDatabaseAsync(() => operation(otherIdentifier, identifier, otherIdentifier), counter);
            await AssertUsesDatabaseAsync(() => operation(otherIdentifier, otherIdentifier, identifier), counter);
        }
    }

    private static async Task AssertUsesDatabaseAsync(
        Func<Task> operation,
        DatabaseCommandCounterInterceptor counter)
    {
        var previousCount = counter.CommandCount;
        try
        {
            await operation();
        }
        catch (UserNotFoundException)
        {
            // A syntactically valid but absent account follows normal ownership checks.
        }
        Assert.That(counter.CommandCount, Is.GreaterThan(previousCount));
    }
}
