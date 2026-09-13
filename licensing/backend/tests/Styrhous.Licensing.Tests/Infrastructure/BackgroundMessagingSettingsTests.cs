using System.Globalization;
using Microsoft.Extensions.Configuration;
using Styrhous.Licensing.Infrastructure.Messaging;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BackgroundMessagingSettingsTests
{
    [Test]
    public void PostgresSettingsRequireOnlyTheQueueName()
    {
        var settings = BackgroundMessagingSettings.From(
            Configuration(
                ("Messaging:QueueName", "styrhous-licensing")));

        Assert.Multiple(() =>
        {
            Assert.That(settings.QueueName, Is.EqualTo("styrhous-licensing"));
            Assert.That(settings.ErrorQueueName, Is.EqualTo("styrhous-licensing-error"));
            Assert.That(settings.MaximumParallelism, Is.EqualTo(4));
        });
    }

    [Test]
    public void ExplicitErrorQueueAndParallelismAreAccepted()
    {
        var settings = BackgroundMessagingSettings.From(
            Configuration(
                ("Messaging:QueueName", "styrhous-licensing"),
                ("Messaging:ErrorQueueName", "styrhous-licensing-dlq"),
                ("Messaging:MaximumParallelism", "8")));

        Assert.Multiple(() =>
        {
            Assert.That(settings.ErrorQueueName, Is.EqualTo("styrhous-licensing-dlq"));
            Assert.That(settings.MaximumParallelism, Is.EqualTo(8));
        });
    }

    [Test]
    public static void MissingQueueNameIsRejected()
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:QueueName", ""))),
            Throws.InvalidOperationException);
    }

    [TestCase(0)]
    [TestCase(33)]
    public void UnsafeParallelismIsRejected(int maximumParallelism)
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:QueueName", "styrhous-licensing"),
                    ("Messaging:MaximumParallelism", maximumParallelism.ToString(
                        CultureInfo.InvariantCulture)))),
            Throws.InvalidOperationException);
    }

    [Test]
    public void WorkAndErrorQueueMustDiffer()
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:QueueName", "styrhous-licensing"),
                    ("Messaging:ErrorQueueName", "styrhous-licensing"))),
            Throws.InvalidOperationException.With.Message.Contains("must differ"));
    }

    [Test]
    public void DerivedErrorQueueMustFitThePortableLimit()
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:QueueName", new string('q', 80)))),
            Throws.InvalidOperationException.With.Message.Contains("ErrorQueueName"));
    }

    private static IConfiguration Configuration(
        params (string Key, string? Value)[] values)
    {
        return new ConfigurationBuilder()
            .AddInMemoryCollection(
                values.ToDictionary(value => value.Key, value => value.Value))
            .Build();
    }
}
