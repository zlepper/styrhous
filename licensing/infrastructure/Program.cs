using System.Text.Json;
using Pulumi;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure;

public static class Program
{
    public static Task<int> Main()
    {
        return Deployment.RunAsync<LicensingInfrastructure>();
    }
}

public sealed partial class LicensingInfrastructure : Stack
{
    private const string DatabaseName = "styrhous_licensing";
    private const string DatabaseMasterUsername = "styrhous";
    private const string DatabaseRuntimeUsername = "styrhous_runtime";
    internal const string ApiFailureRateExpression =
        "IF(requests > 0, 100 * errors / requests, 0)";

    private static readonly string[] AllIpv4 = ["0.0.0.0/0"];
    private static readonly string[] LicensingVpcIpv4 = ["10.42.0.0/16"];
    private static readonly string[] SecretReadActions = ["secretsmanager:GetSecretValue"];
    private static readonly string[] FargateCompatibility = ["FARGATE"];
    private static readonly string[] Tls12Protocols = ["TLSv1.2"];
    private static readonly string[] QueueSendActions =
    [
        "sqs:GetQueueUrl",
        "sqs:SendMessageBatch",
    ];
    private static readonly string[] QueueWorkerActions =
    [
        "sqs:DeleteMessage",
        "sqs:DeleteMessageBatch",
        "sqs:ChangeMessageVisibility",
        "sqs:GetQueueUrl",
        "sqs:ReceiveMessage",
        "sqs:SendMessage",
        "sqs:SendMessageBatch",
    ];
    private static readonly string[] SesSendActions = ["ses:SendEmail"];
    private static readonly string[] MaintenanceLambdaCommand = ["maintenance-lambda"];
    private static readonly string[] EcrLambdaPullActions =
    [
        "ecr:BatchGetImage",
        "ecr:GetDownloadUrlForLayer",
    ];
    private static readonly JsonSerializerOptions CaseInsensitiveJson = new()
    {
        PropertyNameCaseInsensitive = true,
    };

    public LicensingInfrastructure()
    {
        var config = new Config();
        var environment = Deployment.Instance.StackName;
        var apiImageUri = config.Require("apiImageUri");
        var workerImageUri = config.Require("workerImageUri");
        var databasePassword = config.RequireSecret("databasePassword");
        var databaseRuntimePassword = config.RequireSecret("databaseRuntimePassword");
        var hostedOrigin = ReadHostedOrigin(config);
        var customDomain = ReadCustomDomain(config, hostedOrigin);
        var maximumWorkerTasks = ReadMaximumWorkerTasks(config);
        var alertEmailAddress = config.Require("alertEmailAddress");

        var network = CreateNetwork(environment);
        var database = CreateDatabase(environment, network, databasePassword);
        var queues = CreateQueues(environment);
        var repositories = CreateRepositories(environment);
        var roles = CreateRoles(environment);
        var logs = CreateLogs(environment);
        var invitationIdentity = new Aws.SesV2.EmailIdentity("licensing-invitations", new()
        {
            EmailIdentityDetails = config.Require("invitationFromAddress"),
            Tags = Tags(environment),
        });
        var databaseMasterCredentials = CreateSecret(
            "licensing-database-master-credentials",
            environment,
            BuildDatabaseCredentials(DatabaseMasterUsername, databasePassword));
        var databaseRuntimeCredentials = CreateSecret(
            "licensing-database-runtime-credentials",
            environment,
            BuildDatabaseCredentials(DatabaseRuntimeUsername, databaseRuntimePassword));
        var proxy = CreateDatabaseProxy(
            environment,
            network,
            database,
            databaseMasterCredentials,
            databaseRuntimeCredentials,
            roles.Proxy);
        var runtimeSecrets = CreateRuntimeSecrets(
            environment,
            config,
            hostedOrigin,
            proxy.Endpoint,
            databasePassword,
            databaseRuntimePassword);
        var api = CreateApi(
            environment,
            apiImageUri,
            network,
            queues,
            runtimeSecrets.Api,
            roles.Api,
            roles.ApiVpcPolicy,
            repositories.ApiLambdaPolicy,
            logs.Api);
        var compute = CreateWorkerCompute(
            environment,
            workerImageUri,
            network,
            queues,
            runtimeSecrets.Worker,
            runtimeSecrets.Migration,
            runtimeSecrets.Provisioning,
            roles,
            logs,
            invitationIdentity);
        var maintenance = CreateMaintenanceApi(
            environment,
            workerImageUri,
            network,
            queues,
            runtimeSecrets.Maintenance,
            roles,
            repositories.WorkerLambdaPolicy,
            logs.Maintenance);
        ConfigureWorkerScaling(
            environment,
            compute.Cluster,
            compute.Service,
            queues.Work,
            maximumWorkerTasks);
        CreateMaintenanceSchedule(environment, maintenance);
        var portal = CreatePortal(environment, api.ApiEndpoint, customDomain);
        var observability = CreateObservability(
            environment,
            api,
            maintenance,
            compute.Cluster,
            compute.Service,
            queues,
            database,
            logs,
            alertEmailAddress);

        PortalBucketName = portal.Bucket.Id;
        PortalDistributionId = portal.Distribution.Id;
        HostedOrigin = customDomain is { } domain
            ? Output.Create($"https://{domain.DomainName}")
            : Output.Format($"https://{portal.Distribution.DomainName}");
        ApiRepositoryUrl = repositories.Api.RepositoryUrl;
        WorkerRepositoryUrl = repositories.Worker.RepositoryUrl;
        MigrationTaskDefinitionArn = compute.MigrationTask.Arn;
        ProvisioningTaskDefinitionArn = compute.ProvisioningTask.Arn;
        MaintenanceFunctionName = maintenance.Name;
        ClusterArn = compute.Cluster.Arn;
        WorkerServiceName = compute.Service.Name;
        WorkQueueUrl = queues.Work.Url;
        DeadLetterQueueUrl = queues.DeadLetter.Url;
        DeadLetterAlarmName = observability.DeadLetterAlarm.Name;
        AlertTopicArn = observability.AlertTopic.Arn;
        PrivateSubnetIds = Output.All(network.PrivateSubnets.Select(subnet => subnet.Id))
            .Apply(values => values.ToArray());
        ApplicationSecurityGroupId = network.ApplicationSecurityGroup.Id;
        OAuthCallbackUrls = HostedOrigin.Apply(origin => new[]
        {
            $"{origin}/auth/provider-callback/github",
            $"{origin}/auth/provider-callback/google",
            $"{origin}/auth/provider-callback/microsoft",
        });
    }

    [Output] public Output<string> PortalBucketName { get; set; } = null!;
    [Output] public Output<string> PortalDistributionId { get; set; } = null!;
    [Output] public Output<string> HostedOrigin { get; set; } = null!;
    [Output] public Output<string> ApiRepositoryUrl { get; set; } = null!;
    [Output] public Output<string> WorkerRepositoryUrl { get; set; } = null!;
    [Output] public Output<string> ProvisioningTaskDefinitionArn { get; set; } = null!;
    [Output] public Output<string> MigrationTaskDefinitionArn { get; set; } = null!;
    [Output] public Output<string> MaintenanceFunctionName { get; set; } = null!;
    [Output] public Output<string> ClusterArn { get; set; } = null!;
    [Output] public Output<string> WorkerServiceName { get; set; } = null!;
    [Output] public Output<string> WorkQueueUrl { get; set; } = null!;
    [Output] public Output<string> DeadLetterQueueUrl { get; set; } = null!;
    [Output] public Output<string> DeadLetterAlarmName { get; set; } = null!;
    [Output] public Output<string> AlertTopicArn { get; set; } = null!;
    [Output] public Output<string[]> PrivateSubnetIds { get; set; } = null!;
    [Output] public Output<string> ApplicationSecurityGroupId { get; set; } = null!;
    [Output] public Output<string[]> OAuthCallbackUrls { get; set; } = null!;
}
