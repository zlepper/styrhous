using System.Globalization;
using System.Text.RegularExpressions;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal enum BackgroundMessagingTransport
{
    RabbitMq,
    AmazonSqs,
}

internal sealed partial class BackgroundMessagingSettings
{
    public const string SectionName = "Messaging";

    private const int DefaultMaximumParallelism = 4;

    private BackgroundMessagingSettings(
        BackgroundMessagingTransport transport,
        string queueName,
        string errorQueueName,
        int maximumParallelism,
        string? rabbitMqConnectionString,
        string? amazonSqsRegion,
        bool createAmazonSqsQueues)
    {
        Transport = transport;
        QueueName = queueName;
        ErrorQueueName = errorQueueName;
        MaximumParallelism = maximumParallelism;
        RabbitMqConnectionString = rabbitMqConnectionString;
        AmazonSqsRegion = amazonSqsRegion;
        CreateAmazonSqsQueues = createAmazonSqsQueues;
    }

    public BackgroundMessagingTransport Transport { get; }

    public string QueueName { get; }

    public string ErrorQueueName { get; }

    public int MaximumParallelism { get; }

    public string? RabbitMqConnectionString { get; }

    public string? AmazonSqsRegion { get; }

    public bool CreateAmazonSqsQueues { get; }

    public static bool ApiOutboxForwardingEnabled(IConfiguration configuration)
    {
        return configuration.GetSection(SectionName).GetValue(
            "ApiOutboxForwardingEnabled",
            defaultValue: true);
    }

    public static string QueueNameFrom(IConfiguration configuration)
    {
        return RequireQueueName(configuration.GetSection(SectionName)["QueueName"], "QueueName");
    }

    public static BackgroundMessagingSettings From(IConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        var section = configuration.GetSection(SectionName);
        var transport = section["Transport"]?.Trim().ToLowerInvariant() switch
        {
            "rabbitmq" => BackgroundMessagingTransport.RabbitMq,
            "amazonsqs" => BackgroundMessagingTransport.AmazonSqs,
            _ => throw ConfigurationError(
                $"{SectionName}:Transport must be RabbitMq or AmazonSqs."),
        };
        var queueName = QueueNameFrom(configuration);
        var errorQueueName = RequireQueueName(
            string.IsNullOrWhiteSpace(section["ErrorQueueName"])
                ? $"{queueName}-error"
                : section["ErrorQueueName"],
            "ErrorQueueName");
        if (string.Equals(queueName, errorQueueName, StringComparison.Ordinal))
        {
            throw ConfigurationError(
                $"{SectionName}:ErrorQueueName must differ from {SectionName}:QueueName.");
        }
        var maximumParallelism = ParseMaximumParallelism(
            section["MaximumParallelism"]);
        return transport switch
        {
            BackgroundMessagingTransport.RabbitMq => new BackgroundMessagingSettings(
                transport,
                queueName,
                errorQueueName,
                maximumParallelism,
                RequireRabbitMqConnectionString(
                    section["RabbitMq:ConnectionString"]),
                amazonSqsRegion: null,
                createAmazonSqsQueues: false),
            BackgroundMessagingTransport.AmazonSqs => new BackgroundMessagingSettings(
                transport,
                queueName,
                errorQueueName,
                maximumParallelism,
                rabbitMqConnectionString: null,
                RequireValue(section["AmazonSqs:Region"], "AmazonSqs:Region"),
                section.GetValue("AmazonSqs:CreateQueues", defaultValue: false)),
            _ => throw new ArgumentOutOfRangeException(nameof(configuration)),
        };
    }

    public override string ToString()
    {
        return $"{Transport} queue {QueueName} with maximum parallelism {MaximumParallelism}";
    }

    private static string RequireQueueName(string? value, string key)
    {
        var queueName = RequireValue(value, key);
        if (!PortableQueueName().IsMatch(queueName))
        {
            throw ConfigurationError(
                $"{SectionName}:{key} must contain 1-80 letters, digits, hyphens, or underscores.");
        }

        return queueName;
    }

    private static string RequireRabbitMqConnectionString(string? value)
    {
        var connectionString = RequireValue(value, "RabbitMq:ConnectionString");
        if (!Uri.TryCreate(connectionString, UriKind.Absolute, out var uri)
            || uri.Scheme is not "amqp" and not "amqps")
        {
            throw ConfigurationError(
                $"{SectionName}:RabbitMq:ConnectionString must be an AMQP URI.");
        }

        return connectionString;
    }

    private static string RequireValue(string? value, string key)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw ConfigurationError($"{SectionName}:{key} is required.");
        }

        return value.Trim();
    }

    private static int ParseMaximumParallelism(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return DefaultMaximumParallelism;
        }

        if (!int.TryParse(
                value,
                NumberStyles.None,
                CultureInfo.InvariantCulture,
                out var maximumParallelism)
            || maximumParallelism is < 1 or > 32)
        {
            throw ConfigurationError(
                $"{SectionName}:MaximumParallelism must be between 1 and 32.");
        }

        return maximumParallelism;
    }

    private static InvalidOperationException ConfigurationError(string message)
    {
        return new(message);
    }

    [GeneratedRegex("^[A-Za-z0-9_-]{1,80}$", RegexOptions.CultureInvariant)]
    private static partial Regex PortableQueueName();
}
