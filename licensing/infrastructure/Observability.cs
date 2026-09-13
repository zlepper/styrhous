using System.Text.Json;
using Pulumi;
using Styrhous.Licensing.Operations;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure;

public sealed partial class LicensingInfrastructure
{
    private static Observability CreateObservability(
        string environment,
        ApiResources api,
        Aws.Lambda.Function maintenance,
        Aws.Ecs.Cluster cluster,
        Aws.Ecs.Service worker,
        Queues queues,
        Aws.Rds.Instance database,
        Logs logs,
        string alertEmailAddress)
    {
        var customMetricNamespace = $"Styrhous/Licensing/{environment}";
        var topic = new Aws.Sns.Topic("licensing-alerts", new()
        {
            Tags = Tags(environment),
        });
        _ = new Aws.Sns.TopicSubscription("licensing-alert-email", new()
        {
            Topic = topic.Arn,
            Protocol = "email",
            Endpoint = alertEmailAddress,
        });
        _ = LambdaErrorAlarm(
            "licensing-api-errors",
            environment,
            topic,
            api.Function.Name);
        _ = LambdaErrorAlarm(
            "licensing-maintenance-errors",
            environment,
            topic,
            maintenance.Name);
        _ = new Aws.CloudWatch.MetricAlarm("licensing-api-failure-rate", new()
        {
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 1,
            EvaluationPeriods = 1,
            TreatMissingData = "notBreaching",
            MetricQueries = new[]
            {
                new Aws.CloudWatch.Inputs.MetricAlarmMetricQueryArgs
                {
                    Id = "rate",
                    Label = "API 5xx percentage",
                    Expression = ApiFailureRateExpression,
                    ReturnData = true,
                },
                ApiMetric("errors", "5xx", api.Gateway),
                ApiMetric("requests", "Count", api.Gateway),
            },
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        var deadLetterAlarm = new Aws.CloudWatch.MetricAlarm("licensing-dead-letter", new()
        {
            Namespace = "AWS/SQS",
            MetricName = "ApproximateNumberOfMessagesVisible",
            Dimensions = queues.DeadLetter.Name.Apply(name => new Dictionary<string, string>
            {
                ["QueueName"] = name,
            }),
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 1,
            Period = 60,
            Statistic = "Maximum",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-database-storage", new()
        {
            Namespace = "AWS/RDS",
            MetricName = "FreeStorageSpace",
            Dimensions = database.Identifier.Apply(identifier => new Dictionary<string, string>
            {
                ["DBInstanceIdentifier"] = identifier,
            }),
            ComparisonOperator = "LessThanThreshold",
            Threshold = 2_147_483_648,
            EvaluationPeriods = 1,
            Period = 300,
            Statistic = "Average",
            TreatMissingData = "breaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-database-memory", new()
        {
            Namespace = "AWS/RDS",
            MetricName = "FreeableMemory",
            Dimensions = database.Identifier.Apply(identifier => new Dictionary<string, string>
            {
                ["DBInstanceIdentifier"] = identifier,
            }),
            ComparisonOperator = "LessThanThreshold",
            Threshold = 134_217_728,
            EvaluationPeriods = 3,
            Period = 300,
            Statistic = "Average",
            TreatMissingData = "breaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = QueueAlarm(
            "licensing-work-visible",
            environment,
            topic,
            queues.Work,
            "ApproximateNumberOfMessagesVisible",
            threshold: 100);
        _ = QueueAlarm(
            "licensing-work-in-flight",
            environment,
            topic,
            queues.Work,
            "ApproximateNumberOfMessagesNotVisible",
            threshold: 100);
        _ = QueueAlarm(
            "licensing-work-oldest",
            environment,
            topic,
            queues.Work,
            "ApproximateAgeOfOldestMessage",
            threshold: 900);
        _ = new Aws.CloudWatch.MetricAlarm("licensing-worker-pending", new()
        {
            Namespace = "ECS/ContainerInsights",
            MetricName = "PendingTaskCount",
            Dimensions = Output.Tuple(cluster.Name, worker.Name).Apply(values =>
                new Dictionary<string, string>
                {
                    ["ClusterName"] = values.Item1,
                    ["ServiceName"] = values.Item2,
                }),
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 3,
            Period = 300,
            Statistic = "Maximum",
            TreatMissingData = "notBreaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-worker-not-running", new()
        {
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 3,
            DatapointsToAlarm = 3,
            TreatMissingData = "notBreaching",
            MetricQueries = new[]
            {
                new Aws.CloudWatch.Inputs.MetricAlarmMetricQueryArgs
                {
                    Id = "gap",
                    Label = "Desired worker tasks not running",
                    Expression = "desired - running",
                    ReturnData = true,
                },
                EcsServiceMetric("desired", "DesiredTaskCount", cluster, worker),
                EcsServiceMetric("running", "RunningTaskCount", cluster, worker),
            },
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.LogMetricFilter("licensing-worker-failures", new()
        {
            LogGroupName = logs.Worker.Name,
            Pattern = "{ $.LogLevel = \"Error\" || $.LogLevel = \"Critical\" }",
            MetricTransformation = new Aws.CloudWatch.Inputs.LogMetricFilterMetricTransformationArgs
            {
                Name = "WorkerFailures",
                Namespace = customMetricNamespace,
                Value = "1",
                DefaultValue = "0",
            },
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-worker-failures", new()
        {
            Namespace = customMetricNamespace,
            MetricName = "WorkerFailures",
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 1,
            Period = 300,
            Statistic = "Sum",
            TreatMissingData = "notBreaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.LogMetricFilter("licensing-stripe-webhook-failures", new()
        {
            LogGroupName = logs.Api.Name,
            Pattern = $"{{ $.EventId = {LicensingOperationalMetrics.StripeWebhookFailureEventId} }}",
            MetricTransformation = new Aws.CloudWatch.Inputs.LogMetricFilterMetricTransformationArgs
            {
                Name = "StripeWebhookFailures",
                Namespace = customMetricNamespace,
                Value = "1",
            },
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-stripe-webhook-failures", new()
        {
            Namespace = customMetricNamespace,
            MetricName = "StripeWebhookFailures",
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 1,
            Period = 300,
            Statistic = "Sum",
            TreatMissingData = "notBreaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.LogMetricFilter("licensing-oldest-outbox", new()
        {
            LogGroupName = logs.Maintenance.Name,
            Pattern = $"{{ $.EventId = {LicensingOperationalMetrics.OldestOutboxAgeEventId} }}",
            MetricTransformation = new Aws.CloudWatch.Inputs.LogMetricFilterMetricTransformationArgs
            {
                Name = "OldestOutboxAgeSeconds",
                Namespace = customMetricNamespace,
                Value = $"$.State.{LicensingOperationalMetrics.OldestOutboxAgeStateName}",
                Unit = "Seconds",
            },
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-oldest-outbox", new()
        {
            Namespace = customMetricNamespace,
            MetricName = "OldestOutboxAgeSeconds",
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 900,
            EvaluationPeriods = 1,
            Period = 300,
            Statistic = "Maximum",
            TreatMissingData = "breaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.LogMetricFilter("licensing-maintenance-failures", new()
        {
            LogGroupName = logs.Maintenance.Name,
            Pattern = "{ $.LogLevel = \"Error\" || $.LogLevel = \"Critical\" }",
            MetricTransformation = new Aws.CloudWatch.Inputs.LogMetricFilterMetricTransformationArgs
            {
                Name = "MaintenanceFailures",
                Namespace = customMetricNamespace,
                Value = "1",
            },
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-maintenance-failures", new()
        {
            Namespace = customMetricNamespace,
            MetricName = "MaintenanceFailures",
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 1,
            Period = 300,
            Statistic = "Sum",
            TreatMissingData = "notBreaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.Dashboard("licensing", new()
        {
            DashboardName = $"styrhous-licensing-{environment}",
            DashboardBody = Output.Tuple(
                api.Function.Name,
                maintenance.Name,
                cluster.Name,
                worker.Name,
                queues.Work.Name).Apply(values =>
                JsonSerializer.Serialize(new
                {
                    widgets = new object[]
                    {
                        MetricWidget("Lambda errors", "AWS/Lambda", "Errors", "FunctionName", values.Item1),
                        MetricWidget("Maintenance errors", "AWS/Lambda", "Errors", "FunctionName", values.Item2),
                        MetricWidget(
                            "Worker desired tasks",
                            "ECS/ContainerInsights",
                            "DesiredTaskCount",
                            "ClusterName",
                            values.Item3,
                            "ServiceName",
                            values.Item4),
                        MetricWidget("Queued work", "AWS/SQS", "ApproximateNumberOfMessagesVisible", "QueueName", values.Item5),
                    },
                })),
        });
        return new Observability(deadLetterAlarm, topic);
    }

    private static Aws.CloudWatch.MetricAlarm LambdaErrorAlarm(
        string name,
        string environment,
        Aws.Sns.Topic topic,
        Input<string> functionName)
    {
        return new(name, new()
        {
            Namespace = "AWS/Lambda",
            MetricName = "Errors",
            Dimensions = functionName.Apply(value => new Dictionary<string, string>
            {
                ["FunctionName"] = value,
            }),
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 1,
            Period = 300,
            Statistic = "Sum",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
    }

    private static Aws.CloudWatch.Inputs.MetricAlarmMetricQueryArgs ApiMetric(
        string id,
        string metricName,
        Aws.ApiGatewayV2.Api api)
    {
        return new()
        {
            Id = id,
            ReturnData = false,
            Metric = new Aws.CloudWatch.Inputs.MetricAlarmMetricQueryMetricArgs
            {
                Namespace = "AWS/ApiGateway",
                MetricName = metricName,
                Period = 300,
                Stat = "Sum",
                Dimensions = api.Id.Apply(value => new Dictionary<string, string>
                {
                    ["ApiId"] = value,
                }),
            },
        };
    }

    private static Aws.CloudWatch.Inputs.MetricAlarmMetricQueryArgs EcsServiceMetric(
        string id,
        string metricName,
        Aws.Ecs.Cluster cluster,
        Aws.Ecs.Service service)
    {
        return new()
        {
            Id = id,
            ReturnData = false,
            Metric = new Aws.CloudWatch.Inputs.MetricAlarmMetricQueryMetricArgs
            {
                Namespace = "ECS/ContainerInsights",
                MetricName = metricName,
                Period = 60,
                Stat = "Maximum",
                Dimensions = Output.Tuple(cluster.Name, service.Name).Apply(values =>
                    new Dictionary<string, string>
                    {
                        ["ClusterName"] = values.Item1,
                        ["ServiceName"] = values.Item2,
                    }),
            },
        };
    }

    private static Aws.CloudWatch.MetricAlarm QueueAlarm(
        string name,
        string environment,
        Aws.Sns.Topic topic,
        Aws.Sqs.Queue queue,
        string metric,
        double threshold)
    {
        return new(name, new()
        {
            Namespace = "AWS/SQS",
            MetricName = metric,
            Dimensions = queue.Name.Apply(queueName => new Dictionary<string, string>
            {
                ["QueueName"] = queueName,
            }),
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = threshold,
            EvaluationPeriods = 1,
            Period = 300,
            Statistic = "Maximum",
            TreatMissingData = "notBreaching",
            AlarmActions = new[] { topic.Arn },
            Tags = Tags(environment),
        });
    }

    private static object MetricWidget(
        string title,
        string metricNamespace,
        string metric,
        params string[] dimensions)
    {
        return new
        {
            type = "metric",
            width = 8,
            height = 6,
            properties = new
            {
                title,
                region = Aws.Config.Region ?? "eu-west-1",
                metrics = new[]
                {
                    new[] { metricNamespace, metric }.Concat(dimensions).ToArray(),
                },
                period = 300,
                stat = "Sum",
            },
        };
    }
}
