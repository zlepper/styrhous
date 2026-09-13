using System.Text.Json;
using Pulumi;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure;

public sealed partial class LicensingInfrastructure
{
    private static Aws.Iam.RolePolicy AddSecretReadPolicy(
        string name,
        Aws.Iam.Role role,
        Aws.SecretsManager.Secret secret)
    {
        return new Aws.Iam.RolePolicy(name, new()
        {
            Role = role.Name,
            Policy = secret.Arn.Apply(
                arn => AccessPolicy(SecretReadActions, arn)),
        });
    }

    private static Aws.Iam.RolePolicy AddQueueRuntimePolicy(
        string name,
        Aws.Iam.Role role,
        Queues queues,
        bool receiveMessages)
    {
        return new Aws.Iam.RolePolicy(name, new()
        {
            Role = role.Name,
            Policy = receiveMessages
                ? Output.Tuple(queues.Work.Arn, queues.DeadLetter.Arn).Apply(arns =>
                    WorkerQueueAccessPolicy(arns.Item1, arns.Item2))
                : queues.Work.Arn.Apply(arn => AccessPolicy(QueueSendActions, arn)),
        });
    }

    private static Aws.Iam.RolePolicy AddSesSendPolicy(
        string name,
        Aws.Iam.Role role,
        Input<string> identityArn)
    {
        return new Aws.Iam.RolePolicy(name, new()
        {
            Role = role.Name,
            Policy = identityArn.Apply(arn => AccessPolicy(SesSendActions, arn)),
        });
    }

    private static InputMap<string> EnvironmentVariables(
        Queues queues,
        ApplicationSecret secret)
    {
        return new()
        {
            ["DOTNET_ENVIRONMENT"] = "Production",
            ["LICENSING_SECRET_ARN"] = secret.Secret.Arn,
            ["LICENSING_SECRET_VERSION"] = secret.Version.VersionId,
            ["Messaging__Transport"] = "AmazonSqs",
            ["Messaging__QueueName"] = queues.Work.Name,
            ["Messaging__ErrorQueueName"] = queues.DeadLetter.Name,
            ["Messaging__AmazonSqs__Region"] = Aws.Config.Region ?? "eu-west-1",
            ["Messaging__AmazonSqs__CreateQueues"] = "false",
        };
    }

    private static string TrustPolicy(string service)
    {
        return JsonSerializer.Serialize(new
        {
            Version = "2012-10-17",
            Statement = new[]
        {
            new
            {
                Effect = "Allow",
                Principal = new { Service = service },
                Action = "sts:AssumeRole",
            },
        },
        });
    }

    private static string AccessPolicy(string[] actions, object resource)
    {
        return JsonSerializer.Serialize(new
        {
            Version = "2012-10-17",
            Statement = new[] { new { Effect = "Allow", Action = actions, Resource = resource } },
        });
    }

    private static string WorkerQueueAccessPolicy(
        object workQueueResource,
        object deadLetterQueueResource)
    {
        return JsonSerializer.Serialize(new
        {
            Version = "2012-10-17",
            Statement = new[]
            {
                new
                {
                    Effect = "Allow",
                    Action = QueueWorkerActions,
                    Resource = workQueueResource,
                },
                new
                {
                    Effect = "Allow",
                    Action = QueueSendActions,
                    Resource = deadLetterQueueResource,
                },
            },
        });
    }

    private static InputMap<string> Tags(string environment)
    {
        return new()
        {
            ["application"] = "styrhous-licensing",
            ["environment"] = environment,
            ["managed-by"] = "pulumi",
        };
    }

    private sealed record Network(
        Aws.Ec2.Vpc Vpc,
        Aws.Ec2.Subnet[] PublicSubnets,
        Aws.Ec2.Subnet[] PrivateSubnets,
        Aws.Ec2.SecurityGroup ApplicationSecurityGroup,
        Aws.Ec2.SecurityGroup ProxySecurityGroup,
        Aws.Ec2.SecurityGroup DatabaseSecurityGroup);
    private sealed record ApplicationSecret(
        Aws.SecretsManager.Secret Secret,
        Aws.SecretsManager.SecretVersion Version);
    private sealed record RuntimeSecrets(
        ApplicationSecret Api,
        ApplicationSecret Worker,
        ApplicationSecret Maintenance,
        ApplicationSecret Migration,
        ApplicationSecret Provisioning);
    private sealed record AuthenticationProviderConfiguration(
        bool Configured,
        string? ClientId,
        Output<string> Secret);
    private sealed record CustomDomainConfiguration(
        string DomainName,
        string HostedZoneId,
        string CertificateArn);
    private sealed record PreviousCertificateConfiguration(
        string Certificate,
        string CertificatePassword);
    private sealed record Queues(Aws.Sqs.Queue Work, Aws.Sqs.Queue DeadLetter);
    private sealed record Repositories(
        Aws.Ecr.Repository Api,
        Aws.Ecr.Repository Worker,
        Aws.Ecr.RepositoryPolicy ApiLambdaPolicy,
        Aws.Ecr.RepositoryPolicy WorkerLambdaPolicy);
    private sealed record Roles(
        Aws.Iam.Role Api,
        Aws.Iam.Role Worker,
        Aws.Iam.Role Maintenance,
        Aws.Iam.Role Migration,
        Aws.Iam.Role Execution,
        Aws.Iam.Role Proxy,
        Aws.Iam.RolePolicyAttachment ApiVpcPolicy,
        Aws.Iam.RolePolicyAttachment MaintenanceVpcPolicy,
        Aws.Iam.RolePolicyAttachment ExecutionPolicy);
    private sealed record Logs(
        Aws.CloudWatch.LogGroup Api,
        Aws.CloudWatch.LogGroup Worker,
        Aws.CloudWatch.LogGroup Maintenance);
    private sealed record ApiResources(
        Output<string> ApiEndpoint,
        Aws.Lambda.Function Function,
        Aws.ApiGatewayV2.Api Gateway);
    private sealed record Compute(
        Aws.Ecs.Cluster Cluster,
        Aws.Ecs.Service Service,
        Aws.Ecs.TaskDefinition MigrationTask,
        Aws.Ecs.TaskDefinition ProvisioningTask);
    private sealed record Portal(Aws.S3.Bucket Bucket, Aws.CloudFront.Distribution Distribution);
    internal sealed record PortalCacheBehaviors(
        Input<Aws.CloudFront.Inputs.DistributionDefaultCacheBehaviorArgs> Default,
        InputList<Aws.CloudFront.Inputs.DistributionOrderedCacheBehaviorArgs> Ordered);
    private sealed record Observability(
        Aws.CloudWatch.MetricAlarm DeadLetterAlarm,
        Aws.Sns.Topic AlertTopic);
}
