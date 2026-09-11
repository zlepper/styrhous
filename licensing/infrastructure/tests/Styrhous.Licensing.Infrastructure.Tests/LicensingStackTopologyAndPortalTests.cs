using Pulumi;
using Pulumi.Utilities;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure.Tests;

public sealed partial class LicensingStackTests
{
    [Test]
    public async Task PreviewContainsTheProductionComputeAndDataTopology()
    {
        var mocks = new RecordingMocks();

        var resources = await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: true));

        Assert.Multiple(() =>
        {
            Assert.That(resources.OfType<Aws.Rds.Instance>(), Has.Exactly(1).Items);
            Assert.That(resources.OfType<Aws.Rds.Proxy>(), Has.Exactly(1).Items);
            Assert.That(resources.OfType<Aws.Lambda.Function>(), Has.Exactly(2).Items);
            Assert.That(resources.OfType<Aws.Ecs.Service>(), Has.Exactly(1).Items);
            Assert.That(resources.OfType<Aws.Ecs.TaskDefinition>(), Has.Exactly(3).Items);
            Assert.That(resources.OfType<Aws.Sqs.Queue>(), Has.Exactly(2).Items);
            Assert.That(resources.OfType<Aws.SesV2.EmailIdentity>(), Has.Exactly(1).Items);
            Assert.That(resources.OfType<Aws.CloudFront.Distribution>(), Has.Exactly(1).Items);
            Assert.That(resources.OfType<Aws.CloudFront.ResponseHeadersPolicy>(), Has.Exactly(2).Items);
            Assert.That(resources.OfType<Aws.Ec2.SecurityGroup>(), Has.Exactly(3).Items);
            Assert.That(resources.OfType<Aws.SecretsManager.SecretVersion>(), Has.Exactly(7).Items);
            Assert.That(resources.OfType<Aws.CloudWatch.MetricAlarm>(), Has.Exactly(17).Items);
            Assert.That(resources.OfType<Aws.CloudWatch.LogMetricFilter>(), Has.Exactly(4).Items);
            Assert.That(resources.OfType<Aws.Sns.TopicSubscription>(), Has.Exactly(1).Items);
            Assert.That(resources.OfType<Aws.Ecr.RepositoryPolicy>(), Has.Exactly(2).Items);
        });
    }

    [Test]
    public void PreviewRejectsMissingAuthenticationProviders()
    {
        _configuration.Remove("styrhous-licensing:githubClientId");
        _configuration.Remove("styrhous-licensing:githubClientSecret");
        _configuration.Remove("styrhous-licensing:googleClientId");
        _configuration.Remove("styrhous-licensing:googleClientSecret");
        _configuration.Remove("styrhous-licensing:microsoftClientId");
        _configuration.Remove("styrhous-licensing:microsoftClientSecret");
        ApplyConfiguration();

        Assert.That(
            async () => await Deployment.TestAsync<LicensingInfrastructure>(
                new RecordingMocks(),
                Options(isPreview: true)),
            Throws.TypeOf<RunException>()
                .With.Message.Contains("At least one complete external authentication provider"));
    }

    [TestCase("http://license.example.com")]
    [TestCase("https://license.example.com/a-path")]
    [TestCase("not-an-origin")]
    public void PreviewRejectsInvalidHostedOrigins(string hostedOrigin)
    {
        _configuration["styrhous-licensing:hostedOrigin"] = hostedOrigin;
        ApplyConfiguration();

        Assert.That(
            async () => await Deployment.TestAsync<LicensingInfrastructure>(
                new RecordingMocks(),
                Options(isPreview: true)),
            Throws.TypeOf<RunException>()
                .With.Message.Contains("absolute HTTPS origin"));
    }

    [Test]
    public void PreviewRejectsPartialCustomDomainConfiguration()
    {
        _configuration["styrhous-licensing:domainName"] = "license.example.com";
        ApplyConfiguration();

        Assert.That(
            async () => await Deployment.TestAsync<LicensingInfrastructure>(
                new RecordingMocks(),
                Options(isPreview: true)),
            Throws.TypeOf<RunException>()
                .With.Message.Contains("must be configured together"));
    }

    [Test]
    public void PreviewRejectsACustomDomainThatDoesNotMatchTheHostedOrigin()
    {
        AddCustomDomainConfiguration("other.example.com");
        ApplyConfiguration();

        Assert.That(
            async () => await Deployment.TestAsync<LicensingInfrastructure>(
                new RecordingMocks(),
                Options(isPreview: true)),
            Throws.TypeOf<RunException>()
                .With.Message.Contains("must match"));
    }

    [TestCase("0")]
    [TestCase("33")]
    [TestCase("many")]
    public void PreviewRejectsAnInvalidWorkerCapacity(string maximumWorkerTasks)
    {
        _configuration["styrhous-licensing:maximumWorkerTasks"] = maximumWorkerTasks;
        ApplyConfiguration();

        Assert.That(
            async () => await Deployment.TestAsync<LicensingInfrastructure>(
                new RecordingMocks(),
                Options(isPreview: true)),
            Throws.TypeOf<RunException>()
                .With.Message.Contains("between 1 and 32"));
    }

    [Test]
    public async Task MockUpdateCreatesACompleteCustomDomain()
    {
        AddCustomDomainConfiguration("license.example.com");
        ApplyConfiguration();
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var distribution = mocks.Single("aws:cloudfront/distribution:Distribution");
        var dns = mocks.All("aws:route53/record:Record").ToArray();

        Assert.Multiple(() =>
        {
            Assert.That(
                JsonInput(distribution, "aliases").ToString(),
                Does.Contain("license.example.com"));
            Assert.That(
                JsonInput(distribution, "viewerCertificate").ToString(),
                Does.Contain("arn:aws:acm:::certificate/test"));
            Assert.That(dns, Has.Length.EqualTo(2));
            Assert.That(dns.Select(record => record.Inputs["type"]),
                Is.EquivalentTo(DnsRecordTypes));
            Assert.That(dns.Select(record => record.Inputs["zoneId"]),
                Has.All.EqualTo("hosted-zone"));
            Assert.That(dns.Select(record => record.Inputs["name"]),
                Has.All.EqualTo("license.example.com"));
        });
    }

    [Test]
    public async Task MockUpdateWiresProxyAndRuntimePoliciesForProductionTraffic()
    {
        var mocks = new RecordingMocks();

        await Deployment.TestAsync<LicensingInfrastructure>(
            mocks,
            Options(isPreview: false));

        var apiPolicy = JsonInput(
            mocks.Named("aws:iam/rolePolicy:RolePolicy", "api-queues"),
            "policy").GetString()!;
        var workQueue = mocks.Named("aws:sqs/queue:Queue", "licensing-work");
        var redrive = JsonInput(workQueue, "redrivePolicy").GetString()!;
        var integration = mocks.Single("aws:apigatewayv2/integration:Integration");
        var apiFunction = mocks.Named("aws:lambda/function:Function", "licensing-api");
        var apiEnvironment = LambdaEnvironment(apiFunction);
        var repositories = mocks.All("aws:ecr/repository:Repository").ToArray();
        var securityHeaders = mocks.Named(
            "aws:cloudfront/responseHeadersPolicy:ResponseHeadersPolicy",
            "licensing-security-headers");
        var securityConfiguration = JsonInput(
            securityHeaders,
            "securityHeadersConfig");
        var portalHeaders = mocks.Named(
            "aws:cloudfront/responseHeadersPolicy:ResponseHeadersPolicy",
            "licensing-portal-security-headers");
        var apiRepositoryPolicy = JsonInput(
            mocks.Named("aws:ecr/repositoryPolicy:RepositoryPolicy", "licensing-api-lambda"),
            "policy").GetString()!;

        Assert.Multiple(() =>
        {
            Assert.That(
                JsonInput(portalHeaders, "securityHeadersConfig")
                    .GetProperty("contentSecurityPolicy").GetProperty("contentSecurityPolicy").GetString(),
                Is.EqualTo("frame-ancestors 'none'"));
            Assert.That(apiPolicy, Does.Contain("sqs:GetQueueUrl"));
            Assert.That(apiPolicy, Does.Not.Contain("sqs:ChangeMessageVisibility"));
            Assert.That(redrive, Does.Contain("licensing-dead-letter"));
            Assert.That(
                securityConfiguration.GetProperty("referrerPolicy")
                    .GetProperty("referrerPolicy").GetString(),
                Is.EqualTo("strict-origin-when-cross-origin"));
            Assert.That(
                securityConfiguration.GetProperty("contentSecurityPolicy")
                    .GetProperty("contentSecurityPolicy").GetString(),
                Does.Contain("frame-ancestors 'none'"));
            Assert.That(
                securityConfiguration.GetProperty("strictTransportSecurity")
                    .GetProperty("accessControlMaxAgeSec").GetInt32(),
                Is.EqualTo(63_072_000));
            Assert.That(integration.Inputs["integrationMethod"], Is.EqualTo("POST"));
            Assert.That(apiEnvironment["AWS_LWA_ASYNC_INIT"], Is.EqualTo("true"));
            Assert.That(repositories.Select(repository => repository.Inputs["imageTagMutability"]),
                Has.All.EqualTo("IMMUTABLE"));
            Assert.That(apiRepositoryPolicy, Does.Contain("lambda.amazonaws.com"));
            Assert.That(apiRepositoryPolicy, Does.Contain("ecr:BatchGetImage"));
        });
    }

    [Test]
    public async Task ApiCacheBehaviorForwardsTheDesktopAndBrowserProtocolContract()
    {
        var behavior = LicensingInfrastructure.OrderedCacheBehavior(
            "api",
            "/api/*",
            "security-policy-id");
        var path = await OutputUtilities.GetValueAsync(
            behavior.PathPattern.Apply(value => value));
        var policy = await OutputUtilities.GetValueAsync(
            behavior.ResponseHeadersPolicyId.Apply(value => value));
        var defaultTtl = await OutputUtilities.GetValueAsync(
            behavior.DefaultTtl.Apply(value => value));
        var forwarded = await OutputUtilities.GetValueAsync(
            behavior.ForwardedValues.Apply(value => value));
        var headers = await OutputUtilities.GetValueAsync(
            forwarded.Headers.Apply(values => values));
        var cookiePolicy = await OutputUtilities.GetValueAsync(
            forwarded.Cookies.Apply(value => value));
        var cookieForwarding = await OutputUtilities.GetValueAsync(
            cookiePolicy.Forward.Apply(value => value));

        Assert.Multiple(() =>
        {
            Assert.That(path, Is.EqualTo("/api/*"));
            Assert.That(policy, Is.EqualTo("security-policy-id"));
            Assert.That(defaultTtl, Is.Zero);
            Assert.That(headers, Does.Contain("Authorization"));
            Assert.That(headers, Does.Contain("Stripe-Signature"));
            Assert.That(headers, Does.Contain("X-CSRF-TOKEN"));
            Assert.That(cookieForwarding, Is.EqualTo("all"));
        });
    }

    [Test]
    public async Task PortalCacheBehaviorsAttachSecurityHeadersToEveryRoute()
    {
        var behaviors = LicensingInfrastructure.BuildPortalCacheBehaviors(
            "rewrite-function-arn",
            "security-policy-id",
            "portal-policy-id");
        var defaultBehavior = await OutputUtilities.GetValueAsync(
            behaviors.Default.Apply(value => value));
        var orderedBehaviors = await OutputUtilities.GetValueAsync(
            behaviors.Ordered.Apply(values => values));
        var defaultPolicy = await OutputUtilities.GetValueAsync(
            defaultBehavior.ResponseHeadersPolicyId.Apply(value => value));
        var defaultFunctions = await OutputUtilities.GetValueAsync(
            defaultBehavior.FunctionAssociations.Apply(values => values));
        var orderedPolicies = await Task.WhenAll(orderedBehaviors.Select(behavior =>
            OutputUtilities.GetValueAsync(
                behavior.ResponseHeadersPolicyId.Apply(value => value))));
        var orderedPaths = await Task.WhenAll(orderedBehaviors.Select(behavior =>
            OutputUtilities.GetValueAsync(behavior.PathPattern.Apply(value => value))));

        Assert.Multiple(() =>
        {
            Assert.That(defaultPolicy, Is.EqualTo("portal-policy-id"));
            Assert.That(defaultFunctions, Has.Exactly(1).Items);
            Assert.That(orderedPolicies, Has.All.EqualTo("security-policy-id"));
            Assert.That(
                orderedPaths,
                Is.EquivalentTo(ApiPathPatterns));
        });
    }

}
