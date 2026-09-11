using System.Text.Json;
using Pulumi;

namespace Styrhous.Licensing.Infrastructure.Tests;

public sealed partial class LicensingStackTests
{
    [Test]
    public async Task MockUpdateBoundsPublicApiAndMaintenanceConcurrency()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var api = mocks.Named("aws:lambda/function:Function", "licensing-api");
        var maintenance = mocks.Named(
            "aws:lambda/function:Function",
            "licensing-maintenance");
        var stage = mocks.Single("aws:apigatewayv2/stage:Stage");
        var routeSettings = JsonInput(stage, "defaultRouteSettings");
        Assert.Multiple(() =>
        {
            Assert.That(api.Inputs["reservedConcurrentExecutions"], Is.EqualTo(20));
            Assert.That(
                maintenance.Inputs["reservedConcurrentExecutions"],
                Is.EqualTo(1));
            Assert.That(routeSettings.GetProperty("throttlingBurstLimit").GetInt32(),
                Is.EqualTo(50));
            Assert.That(routeSettings.GetProperty("throttlingRateLimit").GetDouble(),
                Is.EqualTo(25));
            Assert.That(routeSettings.GetProperty("detailedMetricsEnabled").GetBoolean(),
                Is.True);
        });
    }

    [Test]
    public async Task MockUpdateConfiguresBoundedScaleToZeroWorkerBehavior()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var queue = mocks.Named("aws:sqs/queue:Queue", "licensing-work");
        var target = mocks.Named(
            "aws:appautoscaling/target:Target",
            "licensing-worker");
        var scaleUp = mocks.Named(
            "aws:appautoscaling/policy:Policy",
            "licensing-worker-up");
        var scaleDown = mocks.Named(
            "aws:cloudwatch/metricAlarm:MetricAlarm",
            "licensing-worker-down");
        var maintenance = mocks.Named(
            "aws:cloudwatch/eventRule:EventRule",
            "licensing-maintenance");
        var maintenanceFunction = mocks.Named(
            "aws:lambda/function:Function",
            "licensing-maintenance");
        var maintenanceEnvironment = LambdaEnvironment(maintenanceFunction);
        var stepAdjustments = JsonInput(
            scaleUp,
            "stepScalingPolicyConfiguration").GetProperty("stepAdjustments");
        var taskDefinitions = mocks.All("aws:ecs/taskDefinition:TaskDefinition")
            .Select(task => JsonInput(task, "containerDefinitions").GetString()!)
            .ToArray();

        Assert.Multiple(() =>
        {
            Assert.That(queue.Inputs["visibilityTimeoutSeconds"], Is.EqualTo(900));
            Assert.That(target.Inputs["minCapacity"], Is.EqualTo(0));
            Assert.That(target.Inputs["maxCapacity"], Is.EqualTo(4));
            Assert.That(stepAdjustments.GetArrayLength(), Is.EqualTo(3));
            Assert.That(stepAdjustments.ToString(), Does.Contain("\"scalingAdjustment\":4"));
            Assert.That(scaleDown.Inputs["evaluationPeriods"], Is.EqualTo(10));
            Assert.That(scaleDown.Inputs["datapointsToAlarm"], Is.EqualTo(10));
            Assert.That(
                maintenance.Inputs["scheduleExpression"],
                Is.EqualTo("rate(5 minutes)"));
            Assert.That(
                JsonInput(maintenanceFunction, "imageConfig").ToString(),
                Does.Contain("maintenance-lambda"));
            Assert.That(
                maintenanceEnvironment["AWS_LWA_PASS_THROUGH_PATH"],
                Is.EqualTo("/internal/lambda/maintenance"));
            Assert.That(
                maintenanceEnvironment["AWS_LWA_READINESS_CHECK_PATH"],
                Is.EqualTo("/health"));
            Assert.That(
                maintenanceEnvironment["AWS_LWA_ERROR_STATUS_CODES"],
                Is.EqualTo("500-599"));
            Assert.That(
                taskDefinitions,
                Has.All.Contains("\"stopTimeout\":120"));
        });
    }

    [Test]
    public async Task MockUpdateHonorsANonDefaultWorkerCapacity()
    {
        _configuration["styrhous-licensing:maximumWorkerTasks"] = "12";
        ApplyConfiguration();
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        Assert.That(
            mocks.Named("aws:appautoscaling/target:Target", "licensing-worker")
                .Inputs["maxCapacity"],
            Is.EqualTo(12));
    }

    [Test]
    public async Task PreMigrationExclusionsMatchTheRecordedComputeResources()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var script = await File.ReadAllTextAsync(
            Path.Combine(AppContext.BaseDirectory, "update-before-migration.sh"));
        var api = mocks.Named("aws:lambda/function:Function", "licensing-api");
        var maintenance = mocks.Named(
            "aws:lambda/function:Function",
            "licensing-maintenance");
        var worker = mocks.Named(
            "aws:ecs/taskDefinition:TaskDefinition",
            "licensing-worker");
        var migration = mocks.Named(
            "aws:ecs/taskDefinition:TaskDefinition",
            "licensing-migration");
        var provisioning = mocks.Named(
            "aws:ecs/taskDefinition:TaskDefinition", "licensing-provisioning");
        using var provisioningDefinition = JsonDocument.Parse(
            JsonInput(provisioning, "containerDefinitions").GetString()!);
        var provisioningContainer = provisioningDefinition.RootElement[0];
        using var migrationDefinition = JsonDocument.Parse(
            JsonInput(migration, "containerDefinitions").GetString()!);
        var migrationContainer = migrationDefinition.RootElement[0];

        Assert.Multiple(() =>
        {
            Assert.That(
                script,
                Does.Contain(
                    $"aws:lambda/function:Function::{api.Name}"));
            Assert.That(
                script,
                Does.Contain(
                    $"aws:lambda/function:Function::{maintenance.Name}"));
            Assert.That(
                script,
                Does.Contain(
                    $"aws:ecs/taskDefinition:TaskDefinition::{worker.Name}"));
            Assert.That(
                migrationContainer.GetProperty("command")[0].GetString(),
                Is.EqualTo("migrate"));
            Assert.That(provisioningContainer.GetProperty("entryPoint")[1].GetString(),
                Is.EqualTo("database-provisioning/Styrhous.Licensing.DatabaseProvisioning.dll"));
            Assert.That(provisioningContainer.GetProperty("command").GetArrayLength(), Is.Zero);
            Assert.That(script, Does.Not.Contain("aws:ecs/taskDefinition:TaskDefinition::licensing-provisioning"));
            Assert.That(
                migrationContainer.GetProperty("image").GetString(),
                Is.EqualTo("registry.example/worker@sha256:worker"));
        });
    }

}
