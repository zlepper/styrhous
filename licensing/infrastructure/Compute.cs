using System.Text.Json;
using Pulumi;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure;

public sealed partial class LicensingInfrastructure
{
    private static ApiResources CreateApi(
        string environment,
        string imageUri,
        Network network,
        Queues queues,
        ApplicationSecret secret,
        Aws.Iam.Role role,
        Aws.Iam.RolePolicyAttachment vpcPolicy,
        Aws.Ecr.RepositoryPolicy repositoryPolicy,
        Aws.CloudWatch.LogGroup logGroup)
    {
        var secretPolicy = AddSecretReadPolicy("api-secret", role, secret.Secret);
        var queuePolicy = AddQueueRuntimePolicy(
            "api-queues",
            role,
            queues,
            receiveMessages: false);
        var variables = EnvironmentVariables(queues, secret);
        variables["AWS_LWA_ASYNC_INIT"] = "true";
        var function = new Aws.Lambda.Function("licensing-api", new()
        {
            PackageType = "Image",
            ImageUri = imageUri,
            Role = role.Arn,
            Timeout = 30,
            MemorySize = 1024,
            ReservedConcurrentExecutions = 20,
            VpcConfig = new Aws.Lambda.Inputs.FunctionVpcConfigArgs
            {
                SubnetIds = network.PrivateSubnets.Select(subnet => subnet.Id).ToArray(),
                SecurityGroupIds = new[] { network.ApplicationSecurityGroup.Id },
            },
            Environment = new Aws.Lambda.Inputs.FunctionEnvironmentArgs
            {
                Variables = variables,
            },
            LoggingConfig = new Aws.Lambda.Inputs.FunctionLoggingConfigArgs
            {
                LogGroup = logGroup.Name,
                LogFormat = "Text",
            },
            Tags = Tags(environment),
        }, new CustomResourceOptions
        {
            DependsOn =
            {
                secret.Version,
                secretPolicy,
                queuePolicy,
                vpcPolicy,
                repositoryPolicy,
            },
        });
        var api = new Aws.ApiGatewayV2.Api("licensing", new()
        {
            ProtocolType = "HTTP",
            Tags = Tags(environment),
        });
        var integration = new Aws.ApiGatewayV2.Integration("licensing", new()
        {
            ApiId = api.Id,
            IntegrationType = "AWS_PROXY",
            IntegrationMethod = "POST",
            IntegrationUri = function.Arn,
            PayloadFormatVersion = "2.0",
        });
        _ = new Aws.ApiGatewayV2.Route("licensing-default", new()
        {
            ApiId = api.Id,
            RouteKey = "$default",
            Target = Output.Format($"integrations/{integration.Id}"),
        });
        _ = new Aws.ApiGatewayV2.Stage("licensing", new()
        {
            ApiId = api.Id,
            Name = "$default",
            AutoDeploy = true,
            DefaultRouteSettings = new Aws.ApiGatewayV2.Inputs.StageDefaultRouteSettingsArgs
            {
                DetailedMetricsEnabled = true,
                ThrottlingBurstLimit = 50,
                ThrottlingRateLimit = 25,
            },
            Tags = Tags(environment),
        });
        _ = new Aws.Lambda.Permission("licensing-api-gateway", new()
        {
            Action = "lambda:InvokeFunction",
            Function = function.Name,
            Principal = "apigateway.amazonaws.com",
            SourceArn = api.ExecutionArn.Apply(arn => $"{arn}/*/*"),
        });
        return new ApiResources(api.ApiEndpoint, function, api);
    }

    private static Compute CreateWorkerCompute(
        string environment,
        string imageUri,
        Network network,
        Queues queues,
        ApplicationSecret workerSecret,
        ApplicationSecret migrationSecret,
        ApplicationSecret provisioningSecret,
        Roles roles,
        Logs logs,
        Aws.SesV2.EmailIdentity invitationIdentity)
    {
        var workerSecretPolicy = AddSecretReadPolicy(
            "worker-secret",
            roles.Worker,
            workerSecret.Secret);
        var workerQueuePolicy = AddQueueRuntimePolicy(
            "worker-queues",
            roles.Worker,
            queues,
            receiveMessages: true);
        var workerEmailPolicy = AddSesSendPolicy(
            "worker-email",
            roles.Worker,
            invitationIdentity.Arn);
        var migrationSecretPolicy = AddSecretReadPolicy(
            "migration-secret",
            roles.Migration,
            migrationSecret.Secret);
        var provisioningSecretPolicy = AddSecretReadPolicy(
            "provisioning-secret",
            roles.Migration,
            provisioningSecret.Secret);
        var cluster = new Aws.Ecs.Cluster("licensing", new()
        {
            Settings = new[]
            {
                new Aws.Ecs.Inputs.ClusterSettingArgs
                {
                    Name = "containerInsights",
                    Value = "enabled",
                },
            },
            Tags = Tags(environment),
        });
        var workerTask = TaskDefinition(
            "worker",
            "worker",
            imageUri,
            environment,
            queues,
            workerSecret,
            roles.Execution,
            roles.Worker,
            logs.Worker,
            [
                workerSecret.Version,
                roles.ExecutionPolicy,
                workerSecretPolicy,
                workerQueuePolicy,
                workerEmailPolicy,
            ]);
        var migrationTask = TaskDefinition(
            "migration",
            "migrate",
            imageUri,
            environment,
            queues,
            migrationSecret,
            roles.Execution,
            roles.Migration,
            logs.Maintenance,
            [
                migrationSecret.Version,
                roles.ExecutionPolicy,
                migrationSecretPolicy,
            ]);
        var provisioningTask = TaskDefinition(
            "provisioning",
            string.Empty,
            imageUri,
            environment,
            queues,
            provisioningSecret,
            roles.Execution,
            roles.Migration,
            logs.Maintenance,
            [provisioningSecret.Version, roles.ExecutionPolicy, provisioningSecretPolicy],
            executable: "database-provisioning/Styrhous.Licensing.DatabaseProvisioning.dll");
        var service = new Aws.Ecs.Service("licensing-worker", new()
        {
            Cluster = cluster.Arn,
            TaskDefinition = workerTask.Arn,
            DesiredCount = 0,
            LaunchType = "FARGATE",
            NetworkConfiguration = new Aws.Ecs.Inputs.ServiceNetworkConfigurationArgs
            {
                AssignPublicIp = false,
                Subnets = network.PrivateSubnets.Select(subnet => subnet.Id).ToArray(),
                SecurityGroups = new[] { network.ApplicationSecurityGroup.Id },
            },
            Tags = Tags(environment),
        }, new CustomResourceOptions
        {
            IgnoreChanges = { "desiredCount" },
        });
        return new Compute(cluster, service, migrationTask, provisioningTask);
    }

    private static Aws.Lambda.Function CreateMaintenanceApi(
        string environment,
        string imageUri,
        Network network,
        Queues queues,
        ApplicationSecret secret,
        Roles roles,
        Aws.Ecr.RepositoryPolicy repositoryPolicy,
        Aws.CloudWatch.LogGroup logGroup)
    {
        var secretPolicy = AddSecretReadPolicy(
            "maintenance-secret",
            roles.Maintenance,
            secret.Secret);
        var queuePolicy = AddQueueRuntimePolicy(
            "maintenance-queues",
            roles.Maintenance,
            queues,
            receiveMessages: false);
        var variables = EnvironmentVariables(queues, secret);
        variables["AWS_LWA_PASS_THROUGH_PATH"] = "/internal/lambda/maintenance";
        variables["AWS_LWA_READINESS_CHECK_PATH"] = "/health";
        variables["AWS_LWA_ASYNC_INIT"] = "true";
        variables["AWS_LWA_ERROR_STATUS_CODES"] = "500-599";
        return new Aws.Lambda.Function("licensing-maintenance", new()
        {
            PackageType = "Image",
            ImageUri = imageUri,
            ImageConfig = new Aws.Lambda.Inputs.FunctionImageConfigArgs
            {
                Commands = MaintenanceLambdaCommand,
            },
            Role = roles.Maintenance.Arn,
            Timeout = 120,
            MemorySize = 512,
            ReservedConcurrentExecutions = 1,
            VpcConfig = new Aws.Lambda.Inputs.FunctionVpcConfigArgs
            {
                SubnetIds = network.PrivateSubnets.Select(subnet => subnet.Id).ToArray(),
                SecurityGroupIds = new[] { network.ApplicationSecurityGroup.Id },
            },
            Environment = new Aws.Lambda.Inputs.FunctionEnvironmentArgs
            {
                Variables = variables,
            },
            LoggingConfig = new Aws.Lambda.Inputs.FunctionLoggingConfigArgs
            {
                LogGroup = logGroup.Name,
                LogFormat = "Text",
            },
            Tags = Tags(environment),
        }, new CustomResourceOptions
        {
            DependsOn =
            {
                secret.Version,
                secretPolicy,
                queuePolicy,
                roles.MaintenanceVpcPolicy,
                repositoryPolicy,
            },
        });
    }

    private static Aws.Ecs.TaskDefinition TaskDefinition(
        string name,
        string command,
        string imageUri,
        string environment,
        Queues queues,
        ApplicationSecret secret,
        Aws.Iam.Role executionRole,
        Aws.Iam.Role taskRole,
        Aws.CloudWatch.LogGroup logGroup,
        Resource[] dependencies,
        string executable = "Styrhous.Licensing.dll")
    {
        return new($"licensing-{name}", new()
        {
            Family = $"styrhous-licensing-{environment}-{name}",
            Cpu = "256",
            Memory = "512",
            NetworkMode = "awsvpc",
            RequiresCompatibilities = FargateCompatibility,
            ExecutionRoleArn = executionRole.Arn,
            TaskRoleArn = taskRole.Arn,
            ContainerDefinitions = Output.Tuple(
                secret.Secret.Arn,
                secret.Version.VersionId,
                logGroup.Name,
                queues.Work.Name,
                queues.DeadLetter.Name).Apply(values => JsonSerializer.Serialize(new[]
                {
                    new
                    {
                        name = "licensing",
                        image = imageUri,
                        essential = true,
                        stopTimeout = 120,
                        entryPoint = new[] { "dotnet", executable },
                        command = string.IsNullOrEmpty(command) ? [] : new[] { command },
                        environment = new[]
                        {
                            new { name = "DOTNET_ENVIRONMENT", value = "Production" },
                            new { name = "LICENSING_SECRET_ARN", value = values.Item1 },
                            new { name = "LICENSING_SECRET_VERSION", value = values.Item2 },
                            new { name = "Messaging__Transport", value = "AmazonSqs" },
                            new { name = "Messaging__QueueName", value = values.Item4 },
                            new { name = "Messaging__ErrorQueueName", value = values.Item5 },
                            new { name = "Messaging__AmazonSqs__Region", value = Aws.Config.Region ?? "eu-west-1" },
                            new { name = "Messaging__AmazonSqs__CreateQueues", value = "false" },
                        },
                        logConfiguration = new
                        {
                            logDriver = "awslogs",
                            options = new Dictionary<string, string>
                            {
                                ["awslogs-group"] = values.Item3,
                                ["awslogs-region"] = Aws.Config.Region ?? "eu-west-1",
                                ["awslogs-stream-prefix"] = name,
                            },
                        },
                    },
                })),
            Tags = Tags(environment),
        }, new CustomResourceOptions
        {
            DependsOn = dependencies,
        });
    }

    private static void ConfigureWorkerScaling(
        string environment,
        Aws.Ecs.Cluster cluster,
        Aws.Ecs.Service service,
        Aws.Sqs.Queue queue,
        int maximumWorkerTasks)
    {
        var target = new Aws.AppAutoScaling.Target("licensing-worker", new()
        {
            MaxCapacity = maximumWorkerTasks,
            MinCapacity = 0,
            ResourceId = Output.Format($"service/{cluster.Name}/{service.Name}"),
            ScalableDimension = "ecs:service:DesiredCount",
            ServiceNamespace = "ecs",
        });
        var scaleUp = new Aws.AppAutoScaling.Policy("licensing-worker-up", new()
        {
            PolicyType = "StepScaling",
            ResourceId = target.ResourceId,
            ScalableDimension = target.ScalableDimension,
            ServiceNamespace = target.ServiceNamespace,
            StepScalingPolicyConfiguration = new Aws.AppAutoScaling.Inputs.PolicyStepScalingPolicyConfigurationArgs
            {
                AdjustmentType = "ChangeInCapacity",
                Cooldown = 30,
                MetricAggregationType = "Maximum",
                StepAdjustments = new[]
                {
                    new Aws.AppAutoScaling.Inputs.PolicyStepScalingPolicyConfigurationStepAdjustmentArgs
                    {
                        MetricIntervalLowerBound = "0",
                        MetricIntervalUpperBound = "10",
                        ScalingAdjustment = 1,
                    },
                    new Aws.AppAutoScaling.Inputs.PolicyStepScalingPolicyConfigurationStepAdjustmentArgs
                    {
                        MetricIntervalLowerBound = "10",
                        MetricIntervalUpperBound = "50",
                        ScalingAdjustment = 2,
                    },
                    new Aws.AppAutoScaling.Inputs.PolicyStepScalingPolicyConfigurationStepAdjustmentArgs
                    {
                        MetricIntervalLowerBound = "50",
                        ScalingAdjustment = 4,
                    },
                },
            },
        });
        var scaleDown = new Aws.AppAutoScaling.Policy("licensing-worker-down", new()
        {
            PolicyType = "StepScaling",
            ResourceId = target.ResourceId,
            ScalableDimension = target.ScalableDimension,
            ServiceNamespace = target.ServiceNamespace,
            StepScalingPolicyConfiguration = new Aws.AppAutoScaling.Inputs.PolicyStepScalingPolicyConfigurationArgs
            {
                AdjustmentType = "ExactCapacity",
                Cooldown = 300,
                MetricAggregationType = "Maximum",
                StepAdjustments = new[]
                {
                    new Aws.AppAutoScaling.Inputs.PolicyStepScalingPolicyConfigurationStepAdjustmentArgs
                    {
                        MetricIntervalUpperBound = "1",
                        ScalingAdjustment = 0,
                    },
                },
            },
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-worker-up", new()
        {
            Namespace = "AWS/SQS",
            MetricName = "ApproximateNumberOfMessagesVisible",
            Dimensions = queue.Name.Apply(name => new Dictionary<string, string>
            {
                ["QueueName"] = name,
            }),
            ComparisonOperator = "GreaterThanThreshold",
            Threshold = 0,
            EvaluationPeriods = 1,
            Period = 60,
            Statistic = "Maximum",
            AlarmActions = new[] { scaleUp.Arn },
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.MetricAlarm("licensing-worker-down", new()
        {
            ComparisonOperator = "LessThanThreshold",
            Threshold = 1,
            EvaluationPeriods = 10,
            DatapointsToAlarm = 10,
            TreatMissingData = "notBreaching",
            MetricQueries = new[]
            {
                new Aws.CloudWatch.Inputs.MetricAlarmMetricQueryArgs
                {
                    Id = "total",
                    Expression = "visible + inflight",
                    ReturnData = true,
                },
                QueueMetric("visible", "ApproximateNumberOfMessagesVisible", queue),
                QueueMetric("inflight", "ApproximateNumberOfMessagesNotVisible", queue),
            },
            AlarmActions = new[] { scaleDown.Arn },
            Tags = Tags(environment),
        });
    }

    private static Aws.CloudWatch.Inputs.MetricAlarmMetricQueryArgs QueueMetric(
        string id,
        string metricName,
        Aws.Sqs.Queue queue)
    {
        return new()
        {
            Id = id,
            ReturnData = false,
            Metric = new Aws.CloudWatch.Inputs.MetricAlarmMetricQueryMetricArgs
            {
                Namespace = "AWS/SQS",
                MetricName = metricName,
                Period = 60,
                Stat = "Maximum",
                Dimensions = queue.Name.Apply(name => new Dictionary<string, string>
                {
                    ["QueueName"] = name,
                }),
            },
        };
    }

    private static void CreateMaintenanceSchedule(
        string environment,
        Aws.Lambda.Function function)
    {
        var schedule = new Aws.CloudWatch.EventRule("licensing-maintenance", new()
        {
            ScheduleExpression = "rate(5 minutes)",
            Tags = Tags(environment),
        });
        _ = new Aws.CloudWatch.EventTarget("licensing-maintenance", new()
        {
            Rule = schedule.Name,
            Arn = function.Arn,
        });
        _ = new Aws.Lambda.Permission("licensing-maintenance-schedule", new()
        {
            Action = "lambda:InvokeFunction",
            Function = function.Name,
            Principal = "events.amazonaws.com",
            SourceArn = schedule.Arn,
        });
    }

}
