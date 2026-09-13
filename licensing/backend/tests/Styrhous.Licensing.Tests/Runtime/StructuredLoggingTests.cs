using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;
using Styrhous.Licensing.Operations;

namespace Styrhous.Licensing.Tests.Runtime;

[TestFixture]
[NonParallelizable]
public sealed class StructuredLoggingTests
{
    private static readonly Action<ILogger, double, Exception?> LogMetric =
        LoggerMessage.Define<double>(
            LogLevel.Information,
            new EventId(
                LicensingOperationalMetrics.OldestOutboxAgeEventId,
                "OldestOutboxAgeObserved"),
            "Oldest pending outbox message age is {OldestOutboxAgeSeconds} seconds.");

    [Test]
    public void ProductionLoggerEmitsTheCloudWatchMetricFilterContract()
    {
        var originalOutput = Console.Out;
        using var output = new StringWriter();
        try
        {
            Console.SetOut(output);
            var services = new ServiceCollection();
            services.AddLogging(Program.ConfigureStructuredLogging);
            using var provider = services.BuildServiceProvider();
            var logger = provider.GetRequiredService<ILoggerFactory>()
                .CreateLogger("LicensingOperationalMetricContract");

            LogMetric(logger, 42d, null);
        }
        finally
        {
            Console.SetOut(originalOutput);
        }

        using var document = JsonDocument.Parse(output.ToString());
        var root = document.RootElement;
        Assert.Multiple(() =>
        {
            Assert.That(
                root.GetProperty("EventId").GetInt32(),
                Is.EqualTo(LicensingOperationalMetrics.OldestOutboxAgeEventId));
            Assert.That(root.GetProperty("LogLevel").GetString(), Is.EqualTo("Information"));
            Assert.That(
                root.GetProperty("State")
                    .GetProperty(LicensingOperationalMetrics.OldestOutboxAgeStateName)
                    .GetDouble(),
                Is.EqualTo(42d));
        });
    }
}
