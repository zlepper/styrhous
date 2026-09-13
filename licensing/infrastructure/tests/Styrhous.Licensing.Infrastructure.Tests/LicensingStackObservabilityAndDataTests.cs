using System.Collections.Immutable;
using Pulumi;
using Pulumi.Testing;

namespace Styrhous.Licensing.Infrastructure.Tests;

public sealed partial class LicensingStackTests
{
    [Test]
    public async Task MockUpdateDefinesEveryOperationalAlarmAndDestination()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var alarmNames = mocks.All("aws:cloudwatch/metricAlarm:MetricAlarm")
            .Select(alarm => alarm.Name)
            .ToArray();
        var subscription = mocks.Single("aws:sns/topicSubscription:TopicSubscription");
        var outboxFilter = mocks.Named(
            "aws:cloudwatch/logMetricFilter:LogMetricFilter",
            "licensing-oldest-outbox");
        var pendingTasks = mocks.Named(
            "aws:cloudwatch/metricAlarm:MetricAlarm",
            "licensing-worker-pending");
        var workerGap = mocks.Named(
            "aws:cloudwatch/metricAlarm:MetricAlarm",
            "licensing-worker-not-running");
        var operationalAlarms = mocks.All("aws:cloudwatch/metricAlarm:MetricAlarm")
            .Where(alarm => alarm.Name is not "licensing-worker-down"
                and not "licensing-worker-up")
            .ToArray();
        var stripeFilter = mocks.Named(
            "aws:cloudwatch/logMetricFilter:LogMetricFilter",
            "licensing-stripe-webhook-failures");
        var workerFilter = mocks.Named(
            "aws:cloudwatch/logMetricFilter:LogMetricFilter",
            "licensing-worker-failures");
        var maintenanceFilter = mocks.Named(
            "aws:cloudwatch/logMetricFilter:LogMetricFilter",
            "licensing-maintenance-failures");
        string[] customMetricResourceNames =
        [
            "licensing-worker-failures",
            "licensing-stripe-webhook-failures",
            "licensing-oldest-outbox",
            "licensing-maintenance-failures",
        ];
        var customMetricFilters = mocks.All("aws:cloudwatch/logMetricFilter:LogMetricFilter")
            .Where(filter => customMetricResourceNames.Contains(filter.Name));
        var customMetricAlarms = mocks.All("aws:cloudwatch/metricAlarm:MetricAlarm")
            .Where(alarm => customMetricResourceNames.Contains(alarm.Name));

        Assert.Multiple(() =>
        {
            Assert.That(subscription.Inputs["protocol"], Is.EqualTo("email"));
            Assert.That(subscription.Inputs["endpoint"], Is.EqualTo("alerts@example.com"));
            Assert.That(alarmNames, Does.Contain("licensing-api-failure-rate"));
            Assert.That(alarmNames, Does.Contain("licensing-api-errors"));
            Assert.That(alarmNames, Does.Contain("licensing-maintenance-errors"));
            Assert.That(alarmNames, Does.Contain("licensing-database-storage"));
            Assert.That(alarmNames, Does.Contain("licensing-database-memory"));
            Assert.That(alarmNames, Does.Contain("licensing-stripe-webhook-failures"));
            Assert.That(alarmNames, Does.Contain("licensing-oldest-outbox"));
            Assert.That(alarmNames, Does.Contain("licensing-work-visible"));
            Assert.That(alarmNames, Does.Contain("licensing-work-in-flight"));
            Assert.That(alarmNames, Does.Contain("licensing-work-oldest"));
            Assert.That(alarmNames, Does.Contain("licensing-worker-pending"));
            Assert.That(alarmNames, Does.Contain("licensing-worker-not-running"));
            Assert.That(alarmNames, Does.Contain("licensing-worker-failures"));
            Assert.That(alarmNames, Does.Contain("licensing-dead-letter"));
            Assert.That(
                JsonInput(pendingTasks, "dimensions").ToString(),
                Does.Contain("ClusterName"));
            Assert.That(
                JsonInput(pendingTasks, "dimensions").ToString(),
                Does.Contain("ServiceName"));
            Assert.That(
                JsonInput(workerGap, "metricQueries").ToString(),
                Does.Contain("DesiredTaskCount"));
            Assert.That(
                JsonInput(workerGap, "metricQueries").ToString(),
                Does.Contain("RunningTaskCount"));
            Assert.That(
                JsonInput(workerGap, "metricQueries").ToString(),
                Does.Contain("ClusterName"));
            Assert.That(
                operationalAlarms,
                Has.All.Matches<MockResourceArgs>(alarm =>
                    JsonInput(alarm, "alarmActions").ToString()
                        .Contains("licensing-alerts", StringComparison.Ordinal)));
            Assert.That(
                LicensingInfrastructure.ApiFailureRateExpression,
                Is.EqualTo("IF(requests > 0, 100 * errors / requests, 0)"));
            Assert.That(
                JsonInput(outboxFilter, "metricTransformation").ToString(),
                Does.Contain("$.State.OldestOutboxAgeSeconds"));
            Assert.That(
                stripeFilter.Inputs["pattern"],
                Is.EqualTo("{ $.EventId = 1001 }"));
            Assert.That(
                outboxFilter.Inputs["pattern"],
                Is.EqualTo("{ $.EventId = 1002 }"));
            Assert.That(
                workerFilter.Inputs["pattern"],
                Is.EqualTo("{ $.LogLevel = \"Error\" || $.LogLevel = \"Critical\" }"));
            Assert.That(
                maintenanceFilter.Inputs["pattern"],
                Is.EqualTo("{ $.LogLevel = \"Error\" || $.LogLevel = \"Critical\" }"));
            Assert.That(
                customMetricFilters,
                Has.All.Matches<MockResourceArgs>(filter =>
                    JsonInput(filter, "metricTransformation")
                        .GetProperty("namespace")
                        .GetString() == "Styrhous/Licensing/test"));
            Assert.That(
                customMetricAlarms,
                Has.All.Matches<MockResourceArgs>(alarm =>
                    Equals(alarm.Inputs["namespace"], "Styrhous/Licensing/test")));
        });
    }
    [Test]
    public async Task MockUpdateProtectsDataAndDefinesPrivateSubnets()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var database = mocks.Single("aws:rds/instance:Instance");
        var proxy = mocks.Single("aws:rds/proxy:Proxy");
        var worker = mocks.Single("aws:ecs/service:Service");
        var privateSubnets = mocks.All("aws:ec2/subnet:Subnet")
            .Where(subnet => Equals(subnet.Inputs["mapPublicIpOnLaunch"], false));
        Assert.Multiple(() =>
        {
            Assert.That(database.Inputs["storageEncrypted"], Is.True);
            Assert.That(database.Inputs["deletionProtection"], Is.True);
            Assert.That(database.Inputs["skipFinalSnapshot"], Is.False);
            Assert.That(proxy.Inputs["requireTls"], Is.True);
            Assert.That(worker.Inputs["desiredCount"], Is.EqualTo(0));
            Assert.That(privateSubnets, Has.Exactly(2).Items);
        });
    }
}
