using System.Globalization;
using Microsoft.Extensions.Configuration;
using Styrhous.Licensing.Infrastructure.Messaging;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BackgroundMessagingSettingsTests
{
    [Test]
    public void RabbitMqSettingsRequireOnlyTheBrokerConnection()
    {
        var settings = BackgroundMessagingSettings.From(
            Configuration(
                ("Messaging:Transport", "RabbitMq"),
                ("Messaging:QueueName", "styrhous-licensing"),
                ("Messaging:RabbitMq:ConnectionString", "amqp://guest:guest@localhost")));

        Assert.Multiple(() =>
        {
            Assert.That(settings.Transport, Is.EqualTo(BackgroundMessagingTransport.RabbitMq));
            Assert.That(settings.QueueName, Is.EqualTo("styrhous-licensing"));
            Assert.That(settings.ErrorQueueName, Is.EqualTo("styrhous-licensing-error"));
            Assert.That(settings.RabbitMqConnectionString, Does.StartWith("amqp://"));
            Assert.That(settings.AmazonSqsRegion, Is.Null);
            Assert.That(settings.MaximumParallelism, Is.EqualTo(4));
        });
    }

    [Test]
    public void AmazonSqsSettingsUseRoleCredentialsAndProvisionedQueues()
    {
        var settings = BackgroundMessagingSettings.From(
            Configuration(
                ("Messaging:Transport", "AmazonSqs"),
                ("Messaging:QueueName", "styrhous-licensing"),
                ("Messaging:ErrorQueueName", "styrhous-licensing-dlq"),
                ("Messaging:AmazonSqs:Region", "eu-west-1"),
                ("Messaging:MaximumParallelism", "8")));

        Assert.Multiple(() =>
        {
            Assert.That(settings.Transport, Is.EqualTo(BackgroundMessagingTransport.AmazonSqs));
            Assert.That(settings.AmazonSqsRegion, Is.EqualTo("eu-west-1"));
            Assert.That(settings.CreateAmazonSqsQueues, Is.False);
            Assert.That(settings.ErrorQueueName, Is.EqualTo("styrhous-licensing-dlq"));
            Assert.That(settings.MaximumParallelism, Is.EqualTo(8));
            Assert.That(settings.RabbitMqConnectionString, Is.Null);
        });
    }

    [TestCase("Kafka")]
    [TestCase("")]
    public void UnsupportedTransportIsRejected(string transport)
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:Transport", transport),
                    ("Messaging:QueueName", "styrhous-licensing"))),
            Throws.InvalidOperationException);
    }

    [TestCase(0)]
    [TestCase(33)]
    public void UnsafeParallelismIsRejected(int maximumParallelism)
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:Transport", "RabbitMq"),
                    ("Messaging:QueueName", "styrhous-licensing"),
                    ("Messaging:RabbitMq:ConnectionString", "amqp://localhost"),
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
                    ("Messaging:Transport", "RabbitMq"),
                    ("Messaging:QueueName", "styrhous-licensing"),
                    ("Messaging:ErrorQueueName", "styrhous-licensing"),
                    ("Messaging:RabbitMq:ConnectionString", "amqp://localhost"))),
            Throws.InvalidOperationException.With.Message.Contains("must differ"));
    }

    [Test]
    public void DerivedErrorQueueMustFitThePortableLimit()
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:Transport", "AmazonSqs"),
                    ("Messaging:QueueName", new string('q', 80)),
                    ("Messaging:AmazonSqs:Region", "eu-west-1"))),
            Throws.InvalidOperationException.With.Message.Contains("ErrorQueueName"));
    }

    [TestCase("rabbitmq", "not-a-uri")]
    [TestCase("rabbitmq", "https://broker.example.com")]
    public void RabbitMqTransportRequiresAmqpUri(
        string transport,
        string connectionString)
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:Transport", transport),
                    ("Messaging:QueueName", "styrhous-licensing"),
                    ("Messaging:RabbitMq:ConnectionString", connectionString))),
            Throws.InvalidOperationException.With.Message.Contains("AMQP URI"));
    }

    [Test]
    public void AmazonSqsTransportRequiresRegion()
    {
        Assert.That(
            () => BackgroundMessagingSettings.From(
                Configuration(
                    ("Messaging:Transport", "AmazonSqs"),
                    ("Messaging:QueueName", "styrhous-licensing"))),
            Throws.InvalidOperationException.With.Message.Contains("AmazonSqs:Region"));
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
