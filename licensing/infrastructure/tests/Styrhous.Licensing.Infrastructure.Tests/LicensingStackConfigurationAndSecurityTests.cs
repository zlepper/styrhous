using System.Text.Json;
using Pulumi;

namespace Styrhous.Licensing.Infrastructure.Tests;

public sealed partial class LicensingStackTests
{
    [Test]
    public async Task MockUpdateKeepsProtectedConfigurationOutOfRuntimeEnvironments()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var apiSecret = mocks.Named(
            "aws:secretsmanager/secretVersion:SecretVersion",
            "licensing-api-configuration");
        var workerSecret = mocks.Named(
            "aws:secretsmanager/secretVersion:SecretVersion",
            "licensing-worker-configuration");
        var maintenanceSecret = mocks.Named(
            "aws:secretsmanager/secretVersion:SecretVersion",
            "licensing-maintenance-configuration");
        var migrationSecret = mocks.Named(
            "aws:secretsmanager/secretVersion:SecretVersion",
            "licensing-migration-configuration");
        var provisioningSecret = mocks.Named(
            "aws:secretsmanager/secretVersion:SecretVersion",
            "licensing-provisioning-configuration");
        var databaseMasterSecret = mocks.Named(
            "aws:secretsmanager/secretVersion:SecretVersion",
            "licensing-database-master-credentials");
        var databaseRuntimeSecret = mocks.Named(
            "aws:secretsmanager/secretVersion:SecretVersion",
            "licensing-database-runtime-credentials");
        using var apiConfiguration = JsonDocument.Parse(
            JsonInput(apiSecret, "secretString").GetString()!);
        using var workerConfiguration = JsonDocument.Parse(
            JsonInput(workerSecret, "secretString").GetString()!);
        using var maintenanceConfiguration = JsonDocument.Parse(
            JsonInput(maintenanceSecret, "secretString").GetString()!);
        using var migrationConfiguration = JsonDocument.Parse(
            JsonInput(migrationSecret, "secretString").GetString()!);
        using var provisioningConfiguration = JsonDocument.Parse(
            JsonInput(provisioningSecret, "secretString").GetString()!);
        using var databaseMasterCredentials = JsonDocument.Parse(
            JsonInput(databaseMasterSecret, "secretString").GetString()!);
        using var databaseRuntimeCredentials = JsonDocument.Parse(
            JsonInput(databaseRuntimeSecret, "secretString").GetString()!);
        var apiEnvironment = LambdaEnvironment(mocks.Named(
            "aws:lambda/function:Function",
            "licensing-api"));
        var maintenanceEnvironment = LambdaEnvironment(mocks.Named(
            "aws:lambda/function:Function",
            "licensing-maintenance"));
        var workerEnvironment = TaskEnvironment(mocks.Named(
            "aws:ecs/taskDefinition:TaskDefinition",
            "licensing-worker"));
        var migrationEnvironment = TaskEnvironment(mocks.Named(
            "aws:ecs/taskDefinition:TaskDefinition",
            "licensing-migration"));

        Assert.Multiple(() =>
        {
            Assert.That(
                apiConfiguration.RootElement
                    .GetProperty("ConnectionStrings")
                    .GetProperty("Licensing")
                    .GetString(),
                Does.Contain("Username=styrhous_runtime"));
            Assert.That(
                apiConfiguration.RootElement
                    .GetProperty("ConnectionStrings")
                    .GetProperty("Licensing")
                    .GetString(),
                Does.Contain("Password=\"test;runtime=password\""));
            Assert.That(
                migrationConfiguration.RootElement
                    .GetProperty("ConnectionStrings")
                    .GetProperty("Licensing")
                    .GetString(),
                Does.Contain("Username=styrhous;Password=\"test;database=password\""));
            Assert.That(
                workerConfiguration.RootElement
                    .GetProperty("ConnectionStrings")
                    .GetProperty("Licensing")
                    .GetString(),
                Does.Contain("Username=styrhous_runtime;Password=\"test;runtime=password\""));
            Assert.That(
                maintenanceConfiguration.RootElement
                    .GetProperty("ConnectionStrings")
                    .GetProperty("Licensing")
                    .GetString(),
                Does.Contain("Username=styrhous_runtime;Password=\"test;runtime=password\""));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("DesktopProtocol")
                    .GetProperty("Certificate").GetString(),
                Is.EqualTo("desktop-certificate"));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("DesktopProtocol")
                    .GetProperty("Issuer").GetString(),
                Is.EqualTo("https://license.example.com"));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("DataProtection")
                    .GetProperty("Certificate").GetString(),
                Is.EqualTo("data-protection-certificate"));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("Stripe")
                    .GetProperty("WebhookSecret").GetString(),
                Is.EqualTo("stripe-webhook-secret"));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("Stripe")
                    .GetProperty("MonthlyPriceId").GetString(),
                Is.EqualTo("price_monthly"));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("Authentication")
                    .GetProperty("GitHub").GetProperty("ClientSecret").GetString(),
                Is.EqualTo("github-secret"));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("Authentication")
                    .GetProperty("Google").GetProperty("ClientSecret").GetString(),
                Is.EqualTo("google-secret"));
            Assert.That(
                apiConfiguration.RootElement.GetProperty("Authentication")
                    .GetProperty("Microsoft").GetProperty("ClientSecret").GetString(),
                Is.EqualTo("microsoft-secret"));
            Assert.That(
                workerConfiguration.RootElement.GetProperty("InvitationEmail")
                    .GetProperty("FromAddress").GetString(),
                Is.EqualTo("licensing@example.com"));
            Assert.That(
                workerConfiguration.RootElement.GetProperty("InfrastructureSmoke")
                    .GetProperty("ProcessingDelaySeconds").GetInt32(),
                Is.EqualTo(720));
            Assert.That(
                apiConfiguration.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ApiConfigurationSections));
            Assert.That(
                workerConfiguration.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(WorkerConfigurationSections));
            Assert.That(
                maintenanceConfiguration.RootElement.EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(ConnectionConfigurationSections));
            Assert.That(
                migrationConfiguration.RootElement.EnumerateObject()
                    .Select(property => property.Name),
                Is.EqualTo(ConnectionConfigurationSections));
            Assert.That(provisioningConfiguration.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EqualTo(ProvisioningConfigurationSections));
            Assert.That(
                provisioningConfiguration.RootElement
                    .GetProperty("DatabaseRuntimeRole")
                    .GetProperty("Username")
                    .GetString(),
                Is.EqualTo("styrhous_runtime"));
            Assert.That(
                provisioningConfiguration.RootElement
                    .GetProperty("DatabaseRuntimeRole")
                    .GetProperty("Password")
                    .GetString(),
                Is.EqualTo("test;runtime=password"));
            Assert.That(
                workerConfiguration.RootElement.TryGetProperty("DesktopProtocol", out _),
                Is.False);
            Assert.That(
                workerConfiguration.RootElement.TryGetProperty("Authentication", out _),
                Is.False);
            Assert.That(
                workerConfiguration.RootElement.GetProperty("Stripe")
                    .TryGetProperty("WebhookSecret", out _),
                Is.False);
            AssertPropertyNames(
                apiConfiguration.RootElement.GetProperty("ConnectionStrings"),
                "Licensing");
            AssertPropertyNames(
                apiConfiguration.RootElement.GetProperty("DesktopProtocol"),
                "Certificate",
                "CertificatePassword",
                "Issuer",
                "PreviousCertificates");
            AssertPropertyNames(
                apiConfiguration.RootElement.GetProperty("DataProtection"),
                "Certificate",
                "CertificatePassword",
                "PreviousCertificates");
            AssertPropertyNames(
                apiConfiguration.RootElement.GetProperty("Stripe"),
                "SecretKey",
                "WebhookSecret",
                "MonthlyPriceId",
                "AnnualPriceId",
                "CheckoutSuccessUrl",
                "CheckoutCancelUrl",
                "CustomerPortalConfigurationId",
                "CustomerPortalReturnUrl");
            AssertPropertyNames(
                apiConfiguration.RootElement.GetProperty("Authentication"),
                "GitHub",
                "Google",
                "Microsoft");
            foreach (var provider in apiConfiguration.RootElement
                .GetProperty("Authentication")
                .EnumerateObject())
            {
                AssertPropertyNames(provider.Value, "ClientId", "ClientSecret");
            }
            AssertPropertyNames(
                workerConfiguration.RootElement.GetProperty("ConnectionStrings"),
                "Licensing");
            AssertPropertyNames(
                workerConfiguration.RootElement.GetProperty("DataProtection"),
                "Certificate",
                "CertificatePassword",
                "PreviousCertificates");
            AssertPropertyNames(
                workerConfiguration.RootElement.GetProperty("Stripe"),
                "SecretKey",
                "MonthlyPriceId",
                "AnnualPriceId");
            AssertPropertyNames(
                workerConfiguration.RootElement.GetProperty("InvitationEmail"),
                "FromAddress",
                "AcceptanceUrl",
                "AmazonSes");
            AssertPropertyNames(
                workerConfiguration.RootElement.GetProperty("InvitationEmail")
                    .GetProperty("AmazonSes"),
                "Region");
            Assert.That(
                workerConfiguration.RootElement.GetProperty("InvitationEmail")
                    .GetProperty("AcceptanceUrl").GetString(),
                Is.EqualTo("https://license.example.com/invitations/accept"));
            Assert.That(
                databaseMasterCredentials.RootElement.EnumerateObject()
                    .Select(property => property.Name),
                Is.EquivalentTo(DatabaseCredentialPropertyNames));
            Assert.That(
                databaseMasterCredentials.RootElement.GetProperty("username").GetString(),
                Is.EqualTo("styrhous"));
            Assert.That(
                databaseMasterCredentials.RootElement.GetProperty("password").GetString(),
                Is.EqualTo("test;database=password"));
            Assert.That(
                databaseRuntimeCredentials.RootElement.EnumerateObject()
                    .Select(property => property.Name),
                Is.EquivalentTo(DatabaseCredentialPropertyNames));
            Assert.That(
                databaseRuntimeCredentials.RootElement.GetProperty("username").GetString(),
                Is.EqualTo("styrhous_runtime"));
            Assert.That(
                databaseRuntimeCredentials.RootElement.GetProperty("password").GetString(),
                Is.EqualTo("test;runtime=password"));
        });

        AssertSafeRuntimeEnvironment(apiEnvironment, "licensing-api-configuration");
        AssertSafeRuntimeEnvironment(workerEnvironment, "licensing-worker-configuration");
        AssertSafeRuntimeEnvironment(
            maintenanceEnvironment,
            "licensing-maintenance-configuration");
        AssertSafeRuntimeEnvironment(
            migrationEnvironment,
            "licensing-migration-configuration");
        AssertSafeRuntimeEnvironment(
            TaskEnvironment(mocks.Named("aws:ecs/taskDefinition:TaskDefinition", "licensing-provisioning")),
            "licensing-provisioning-configuration");
    }

    [Test]
    public async Task MockUpdateUsesSeparateDatabaseAndApplicationSecrets()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var proxy = mocks.Single("aws:rds/proxy:Proxy");
        var proxyPolicy = JsonInput(
            mocks.Named("aws:iam/rolePolicy:RolePolicy", "proxy-secret"),
            "policy").GetString()!;
        var proxyAuthentication = JsonInput(proxy, "auths").ToString();

        Assert.Multiple(() =>
        {
            Assert.That(
                proxyAuthentication,
                Does.Contain("arn:aws:test:::licensing-database-master-credentials"));
            Assert.That(
                proxyAuthentication,
                Does.Contain("arn:aws:test:::licensing-database-runtime-credentials"));
            Assert.That(
                proxyAuthentication,
                Does.Not.Contain("licensing-api-configuration"));
            Assert.That(
                proxyPolicy,
                Does.Contain("arn:aws:test:::licensing-database-master-credentials"));
            Assert.That(
                proxyPolicy,
                Does.Contain("arn:aws:test:::licensing-database-runtime-credentials"));
            Assert.That(
                proxyPolicy,
                Does.Not.Contain("licensing-api-configuration"));
        });
    }

    [Test]
    public async Task MockUpdateAssignsLeastPrivilegeRuntimeRoles()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var worker = mocks.Named(
            "aws:ecs/taskDefinition:TaskDefinition",
            "licensing-worker");
        var migration = mocks.Named(
            "aws:ecs/taskDefinition:TaskDefinition",
            "licensing-migration");
        var maintenance = mocks.Named(
            "aws:lambda/function:Function",
            "licensing-maintenance");
        var apiSecretPolicy = Policy(mocks, "api-secret");
        var apiQueuePolicy = Policy(mocks, "api-queues");
        var workerSecretPolicy = Policy(mocks, "worker-secret");
        var workerQueuePolicy = Policy(mocks, "worker-queues");
        var workerEmailPolicy = Policy(mocks, "worker-email");
        var maintenanceSecretPolicy = Policy(mocks, "maintenance-secret");
        var maintenanceQueuePolicy = Policy(mocks, "maintenance-queues");
        var migrationSecretPolicy = Policy(mocks, "migration-secret");
        var provisioningSecretPolicy = Policy(mocks, "provisioning-secret");
        var scheduleTarget = mocks.Named(
            "aws:cloudwatch/eventTarget:EventTarget",
            "licensing-maintenance");
        using var workerQueuePolicyDocument = JsonDocument.Parse(workerQueuePolicy);
        var workerQueueStatements = workerQueuePolicyDocument.RootElement
            .GetProperty("Statement")
            .EnumerateArray()
            .ToArray();
        var deadLetterStatement = workerQueueStatements.Single(statement =>
            statement.GetProperty("Resource").GetString()!
                .Contains("licensing-dead-letter", StringComparison.Ordinal));
        var workQueueStatement = workerQueueStatements.Single(statement =>
            !statement.GetProperty("Resource").GetString()!
                .Contains("licensing-dead-letter", StringComparison.Ordinal));
        var deadLetterActions = deadLetterStatement
            .GetProperty("Action")
            .EnumerateArray()
            .Select(action => action.GetString())
            .ToArray();
        var workQueueActions = workQueueStatement
            .GetProperty("Action")
            .EnumerateArray()
            .Select(action => action.GetString())
            .ToArray();
        using var apiQueuePolicyDocument = JsonDocument.Parse(apiQueuePolicy);
        var apiQueueActions = apiQueuePolicyDocument.RootElement
            .GetProperty("Statement")[0]
            .GetProperty("Action")
            .EnumerateArray()
            .Select(action => action.GetString())
            .ToArray();
        using var maintenanceQueuePolicyDocument = JsonDocument.Parse(
            maintenanceQueuePolicy);
        var maintenanceQueueActions = maintenanceQueuePolicyDocument.RootElement
            .GetProperty("Statement")[0]
            .GetProperty("Action")
            .EnumerateArray()
            .Select(action => action.GetString())
            .ToArray();

        Assert.Multiple(() =>
        {
            Assert.That(
                worker.Inputs["taskRoleArn"],
                Is.EqualTo("arn:aws:test:::licensing-worker"));
            Assert.That(
                migration.Inputs["taskRoleArn"],
                Is.EqualTo("arn:aws:test:::licensing-migration"));
            Assert.That(
                maintenance.Inputs["role"],
                Is.EqualTo("arn:aws:test:::licensing-maintenance"));
            Assert.That(apiSecretPolicy, Does.Contain("secretsmanager:GetSecretValue"));
            Assert.That(apiSecretPolicy, Does.Not.Contain("sqs:"));
            Assert.That(apiSecretPolicy, Does.Not.Contain("ses:"));
            Assert.That(apiQueueActions, Is.EquivalentTo(ExpectedQueueSendActions));
            Assert.That(apiQueuePolicy, Does.Not.Contain("licensing-dead-letter"));
            Assert.That(apiQueuePolicy, Does.Not.Contain("ses:"));
            Assert.That(workerSecretPolicy, Does.Not.Contain("sqs:"));
            Assert.That(workerQueueStatements, Has.Length.EqualTo(2));
            Assert.That(workQueueActions, Does.Contain("sqs:ReceiveMessage"));
            Assert.That(workQueueActions, Does.Contain("sqs:DeleteMessageBatch"));
            Assert.That(workQueueActions, Does.Contain("sqs:SendMessageBatch"));
            Assert.That(deadLetterActions, Does.Contain("sqs:SendMessageBatch"));
            Assert.That(deadLetterActions, Does.Not.Contain("sqs:ReceiveMessage"));
            Assert.That(deadLetterActions, Does.Not.Contain("sqs:DeleteMessage"));
            Assert.That(deadLetterActions, Does.Not.Contain("sqs:ChangeMessageVisibility"));
            Assert.That(workerEmailPolicy, Does.Contain("ses:SendEmail"));
            Assert.That(workerEmailPolicy, Does.Contain("licensing-invitations"));
            Assert.That(workerEmailPolicy, Does.Not.Contain("\"Resource\":\"*\""));
            Assert.That(maintenanceSecretPolicy, Does.Not.Contain("sqs:"));
            Assert.That(maintenanceQueueActions, Is.EquivalentTo(apiQueueActions));
            Assert.That(maintenanceQueuePolicy, Does.Not.Contain("licensing-dead-letter"));
            Assert.That(migrationSecretPolicy, Does.Not.Contain("sqs:"));
            Assert.That(migrationSecretPolicy, Does.Not.Contain("ses:"));
            Assert.That(provisioningSecretPolicy, Does.Contain("licensing-provisioning-configuration"));
            Assert.That(provisioningSecretPolicy, Does.Not.Contain("sqs:"));
            Assert.That(provisioningSecretPolicy, Does.Not.Contain("ses:"));
            Assert.That(mocks.Named("aws:ecs/taskDefinition:TaskDefinition", "licensing-provisioning").Inputs["taskRoleArn"],
                Is.EqualTo("arn:aws:test:::licensing-migration"));
            Assert.That(
                scheduleTarget.Inputs["arn"],
                Is.EqualTo("arn:aws:test:::licensing-maintenance"));
            Assert.That(
                mocks.Named("aws:iam/rolePolicy:RolePolicy", "api-secret")
                    .Inputs["role"],
                Is.EqualTo("licensing-api"));
            Assert.That(
                mocks.Named("aws:iam/rolePolicy:RolePolicy", "worker-secret")
                    .Inputs["role"],
                Is.EqualTo("licensing-worker"));
            Assert.That(
                mocks.Named("aws:iam/rolePolicy:RolePolicy", "maintenance-secret")
                    .Inputs["role"],
                Is.EqualTo("licensing-maintenance"));
            Assert.That(
                mocks.Named("aws:iam/rolePolicy:RolePolicy", "migration-secret")
                    .Inputs["role"],
                Is.EqualTo("licensing-migration"));
            Assert.That(apiSecretPolicy, Does.Contain("licensing-api-configuration"));
            Assert.That(workerSecretPolicy, Does.Contain("licensing-worker-configuration"));
            Assert.That(
                maintenanceSecretPolicy,
                Does.Contain("licensing-maintenance-configuration"));
            Assert.That(
                migrationSecretPolicy,
                Does.Contain("licensing-migration-configuration"));
            Assert.That(
                string.Join('\n', new[]
                {
                    apiSecretPolicy,
                    workerSecretPolicy,
                    maintenanceSecretPolicy,
                    migrationSecretPolicy,
                    provisioningSecretPolicy,
                }),
                Does.Not.Contain("licensing-database-master-credentials"));
            Assert.That(
                string.Join('\n', new[]
                {
                    apiSecretPolicy,
                    workerSecretPolicy,
                    maintenanceSecretPolicy,
                    migrationSecretPolicy,
                    provisioningSecretPolicy,
                }),
                Does.Not.Contain("licensing-database-runtime-credentials"));
        });
    }

}
