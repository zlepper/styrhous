using System.Globalization;
using System.Text.RegularExpressions;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed partial class BackgroundMessagingSettings
{
    public const string SectionName = "Messaging";

    private const int DefaultMaximumParallelism = 4;

    private BackgroundMessagingSettings(
        string queueName,
        string errorQueueName,
        int maximumParallelism)
    {
        QueueName = queueName;
        ErrorQueueName = errorQueueName;
        MaximumParallelism = maximumParallelism;
    }

    public string QueueName { get; }

    public string ErrorQueueName { get; }

    public int MaximumParallelism { get; }

    public static BackgroundMessagingSettings From(IConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        var section = configuration.GetSection(SectionName);
        var queueName = RequireQueueName(section["QueueName"], "QueueName");
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

        return new BackgroundMessagingSettings(
            queueName,
            errorQueueName,
            ParseMaximumParallelism(section["MaximumParallelism"]));
    }

    public override string ToString()
    {
        return $"PostgreSQL queue {QueueName} with maximum parallelism {MaximumParallelism}";
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
        return new InvalidOperationException(message);
    }

    [GeneratedRegex("^[A-Za-z0-9_-]{1,80}$", RegexOptions.CultureInvariant)]
    private static partial Regex PortableQueueName();
}
