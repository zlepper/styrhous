using System.Text.Json;
using Pulumi;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure;

public sealed partial class LicensingInfrastructure
{
    private static Portal CreatePortal(
        string environment,
        Output<string> apiEndpoint,
        CustomDomainConfiguration? customDomain)
    {
        var bucket = new Aws.S3.Bucket("licensing-portal", new()
        {
            ForceDestroy = false,
            Tags = Tags(environment),
        });
        _ = new Aws.S3.BucketPublicAccessBlock("licensing-portal", new()
        {
            Bucket = bucket.Id,
            BlockPublicAcls = true,
            BlockPublicPolicy = true,
            IgnorePublicAcls = true,
            RestrictPublicBuckets = true,
        });
        var accessControl = new Aws.CloudFront.OriginAccessControl("licensing-portal", new()
        {
            OriginAccessControlOriginType = "s3",
            SigningBehavior = "always",
            SigningProtocol = "sigv4",
        });
        var portalRewrite = new Aws.CloudFront.Function("licensing-portal-routes", new()
        {
            Runtime = "cloudfront-js-2.0",
            Publish = true,
            Code = "function handler(event) { var request = event.request; "
                + "if (!request.uri.includes('.')) request.uri = '/index.html'; "
                + "return request; }",
        });
        using var headerStream = typeof(LicensingInfrastructure).Assembly
            .GetManifestResourceStream("PortalSecurityHeaders")!;
        var portalHeaders = JsonSerializer.Deserialize<Dictionary<string, string>>(headerStream)!;
        var portalSecurityHeaders = CreateSecurityHeaders(
            environment,
            "licensing-portal-security-headers",
            portalHeaders["Content-Security-Policy"]);
        var securityHeaders = CreateSecurityHeaders(
            environment,
            "licensing-security-headers",
            "default-src 'self'; base-uri 'self'; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; img-src 'self' data:; object-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'");
        var cacheBehaviors = BuildPortalCacheBehaviors(
            portalRewrite.Arn,
            securityHeaders.Id,
            portalSecurityHeaders.Id);
        var distribution = new Aws.CloudFront.Distribution("licensing", new()
        {
            Enabled = true,
            IsIpv6Enabled = true,
            DefaultRootObject = "index.html",
            Origins = new[]
            {
                new Aws.CloudFront.Inputs.DistributionOriginArgs
                {
                    OriginId = "portal",
                    DomainName = bucket.BucketRegionalDomainName,
                    OriginAccessControlId = accessControl.Id,
                },
                new Aws.CloudFront.Inputs.DistributionOriginArgs
                {
                    OriginId = "api",
                    DomainName = apiEndpoint.Apply(value => new Uri(value).Host),
                    CustomOriginConfig = new Aws.CloudFront.Inputs.DistributionOriginCustomOriginConfigArgs
                    {
                        HttpPort = 80,
                        HttpsPort = 443,
                        OriginProtocolPolicy = "https-only",
                        OriginSslProtocols = Tls12Protocols,
                    },
                },
            },
            DefaultCacheBehavior = cacheBehaviors.Default,
            OrderedCacheBehaviors = cacheBehaviors.Ordered,
            Restrictions = new Aws.CloudFront.Inputs.DistributionRestrictionsArgs
            {
                GeoRestriction = new Aws.CloudFront.Inputs.DistributionRestrictionsGeoRestrictionArgs
                {
                    RestrictionType = "none",
                },
            },
            ViewerCertificate = customDomain is { } domain
                ? new Aws.CloudFront.Inputs.DistributionViewerCertificateArgs
                {
                    AcmCertificateArn = domain.CertificateArn,
                    SslSupportMethod = "sni-only",
                    MinimumProtocolVersion = "TLSv1.2_2021",
                }
                : new Aws.CloudFront.Inputs.DistributionViewerCertificateArgs
                {
                    CloudfrontDefaultCertificate = true,
                },
            Aliases = customDomain is { } alias
                ? new[] { alias.DomainName }
                : [],
            Tags = Tags(environment),
        }, new CustomResourceOptions
        {
            DependsOn = { securityHeaders },
        });
        _ = new Aws.S3.BucketPolicy("licensing-portal", new()
        {
            Bucket = bucket.Id,
            Policy = Output.Tuple(bucket.Arn, distribution.Arn).Apply(values =>
                JsonSerializer.Serialize(new
                {
                    Version = "2012-10-17",
                    Statement = new[]
                    {
                        new
                        {
                            Effect = "Allow",
                            Principal = new { Service = "cloudfront.amazonaws.com" },
                            Action = "s3:GetObject",
                            Resource = $"{values.Item1}/*",
                            Condition = new
                            {
                                StringEquals = new Dictionary<string, string>
                                {
                                    ["AWS:SourceArn"] = values.Item2,
                                },
                            },
                        },
                    },
                })),
        });
        if (customDomain is { } dns)
        {
            _ = new Aws.Route53.Record("licensing", new()
            {
                ZoneId = dns.HostedZoneId,
                Name = dns.DomainName,
                Type = "A",
                Aliases = new[]
                {
                    new Aws.Route53.Inputs.RecordAliasArgs
                    {
                        Name = distribution.DomainName,
                        ZoneId = distribution.HostedZoneId,
                        EvaluateTargetHealth = false,
                    },
                },
            });
            _ = new Aws.Route53.Record("licensing-ipv6", new()
            {
                ZoneId = dns.HostedZoneId,
                Name = dns.DomainName,
                Type = "AAAA",
                Aliases = new[]
                {
                    new Aws.Route53.Inputs.RecordAliasArgs
                    {
                        Name = distribution.DomainName,
                        ZoneId = distribution.HostedZoneId,
                        EvaluateTargetHealth = false,
                    },
                },
            });
        }
        return new Portal(bucket, distribution);
    }

    internal static Aws.CloudFront.Inputs.DistributionDefaultCacheBehaviorArgs DefaultCacheBehavior(
        string origin,
        bool cache,
        string rewriteFunctionArn,
        string responseHeadersPolicyId)
    {
        return new()
        {
            TargetOriginId = origin,
            ViewerProtocolPolicy = "redirect-to-https",
            AllowedMethods = new[] { "DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT" },
            CachedMethods = new[] { "GET", "HEAD" },
            Compress = true,
            MinTtl = 0,
            DefaultTtl = cache ? 3600 : 0,
            MaxTtl = cache ? 86400 : 0,
            ResponseHeadersPolicyId = responseHeadersPolicyId,
            ForwardedValues = new Aws.CloudFront.Inputs.DistributionDefaultCacheBehaviorForwardedValuesArgs
            {
                QueryString = true,
                Headers = cache ? [] : new[] { "*" },
                Cookies = new Aws.CloudFront.Inputs.DistributionDefaultCacheBehaviorForwardedValuesCookiesArgs
                {
                    Forward = cache ? "none" : "all",
                },
            },
            FunctionAssociations = new[]
        {
            new Aws.CloudFront.Inputs.DistributionDefaultCacheBehaviorFunctionAssociationArgs
            {
                EventType = "viewer-request",
                FunctionArn = rewriteFunctionArn,
            },
        },
        };
    }

    internal static PortalCacheBehaviors BuildPortalCacheBehaviors(
        Input<string> rewriteFunctionArn,
        Input<string> responseHeadersPolicyId,
        Input<string> portalResponseHeadersPolicyId)
    {
        var defaultBehavior = Output.Tuple(rewriteFunctionArn, portalResponseHeadersPolicyId)
            .Apply(values => DefaultCacheBehavior(
                "portal",
                cache: true,
                values.Item1,
                values.Item2));
        var orderedBehaviors = new InputList<
            Aws.CloudFront.Inputs.DistributionOrderedCacheBehaviorArgs>
        {
            responseHeadersPolicyId.Apply(id => OrderedCacheBehavior("api", "/api/*", id)),
            responseHeadersPolicyId.Apply(id => OrderedCacheBehavior("api", "/auth/*", id)),
            responseHeadersPolicyId.Apply(id => OrderedCacheBehavior("api", "/desktop/*", id)),
            responseHeadersPolicyId.Apply(id => OrderedCacheBehavior("api", "/health", id)),
        };
        return new PortalCacheBehaviors(defaultBehavior, orderedBehaviors);
    }

    internal static Aws.CloudFront.Inputs.DistributionOrderedCacheBehaviorArgs OrderedCacheBehavior(
        string origin,
        string pathPattern,
        string responseHeadersPolicyId)
    {
        return new()
        {
            TargetOriginId = origin,
            PathPattern = pathPattern,
            ViewerProtocolPolicy = "redirect-to-https",
            AllowedMethods = new[] { "DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT" },
            CachedMethods = new[] { "GET", "HEAD" },
            Compress = true,
            MinTtl = 0,
            DefaultTtl = 0,
            MaxTtl = 0,
            ResponseHeadersPolicyId = responseHeadersPolicyId,
            ForwardedValues = new Aws.CloudFront.Inputs.DistributionOrderedCacheBehaviorForwardedValuesArgs
            {
                QueryString = true,
                Headers = new[]
            {
                "Accept",
                "Authorization",
                "Content-Type",
                "Origin",
                "Referer",
                "Stripe-Signature",
                "User-Agent",
                "X-CSRF-TOKEN",
            },
                Cookies = new Aws.CloudFront.Inputs.DistributionOrderedCacheBehaviorForwardedValuesCookiesArgs
                {
                    Forward = "all",
                },
            },
        };
    }

    private static Aws.CloudFront.ResponseHeadersPolicy CreateSecurityHeaders(
        string environment,
        string name,
        string contentSecurityPolicy)
    {
        return new(name, new()
        {
            Name = $"styrhous-{environment}-{name}",
            SecurityHeadersConfig = new Aws.CloudFront.Inputs.ResponseHeadersPolicySecurityHeadersConfigArgs
            {
                ContentSecurityPolicy = new Aws.CloudFront.Inputs.ResponseHeadersPolicySecurityHeadersConfigContentSecurityPolicyArgs { ContentSecurityPolicy = contentSecurityPolicy, Override = true },
                ContentTypeOptions = new Aws.CloudFront.Inputs.ResponseHeadersPolicySecurityHeadersConfigContentTypeOptionsArgs { Override = true },
                FrameOptions = new Aws.CloudFront.Inputs.ResponseHeadersPolicySecurityHeadersConfigFrameOptionsArgs { FrameOption = "DENY", Override = true },
                ReferrerPolicy = new Aws.CloudFront.Inputs.ResponseHeadersPolicySecurityHeadersConfigReferrerPolicyArgs { ReferrerPolicy = "strict-origin-when-cross-origin", Override = true },
                StrictTransportSecurity = new Aws.CloudFront.Inputs.ResponseHeadersPolicySecurityHeadersConfigStrictTransportSecurityArgs
                {
                    AccessControlMaxAgeSec = 63_072_000,
                    IncludeSubdomains = true,
                    Preload = true,
                    Override = true,
                },
            },
        });
    }
}
