using System.Text.Json;
using Pulumi;
using Aws = Pulumi.Aws;

namespace Styrhous.Licensing.Infrastructure;

public sealed partial class LicensingInfrastructure
{
    private static Network CreateNetwork(string environment)
    {
        var vpc = new Aws.Ec2.Vpc("licensing", new()
        {
            CidrBlock = "10.42.0.0/16",
            EnableDnsHostnames = true,
            EnableDnsSupport = true,
            Tags = Tags(environment),
        });
        var availabilityZones = Aws.GetAvailabilityZones.Invoke(new()
        {
            State = "available",
        });
        var internetGateway = new Aws.Ec2.InternetGateway("licensing", new()
        {
            VpcId = vpc.Id,
            Tags = Tags(environment),
        });
        var publicRouteTable = new Aws.Ec2.RouteTable("public", new()
        {
            VpcId = vpc.Id,
            Routes = new[]
            {
                new Aws.Ec2.Inputs.RouteTableRouteArgs
                {
                    CidrBlock = "0.0.0.0/0",
                    GatewayId = internetGateway.Id,
                },
            },
            Tags = Tags(environment),
        });
        var publicSubnets = Enumerable.Range(0, 2).Select(index =>
        {
            var subnet = new Aws.Ec2.Subnet($"public-{index + 1}", new()
            {
                VpcId = vpc.Id,
                CidrBlock = $"10.42.{index}.0/24",
                AvailabilityZone = availabilityZones.Apply(zones => zones.Names[index]),
                MapPublicIpOnLaunch = true,
                Tags = Tags(environment),
            });
            _ = new Aws.Ec2.RouteTableAssociation($"public-{index + 1}", new()
            {
                RouteTableId = publicRouteTable.Id,
                SubnetId = subnet.Id,
            });
            return subnet;
        }).ToArray();
        var privateSubnets = Enumerable.Range(0, 2).Select(index =>
        {
            var address = new Aws.Ec2.Eip($"nat-{index + 1}", new()
            {
                Domain = "vpc",
                Tags = Tags(environment),
            });
            var gateway = new Aws.Ec2.NatGateway($"licensing-{index + 1}", new()
            {
                AllocationId = address.Id,
                SubnetId = publicSubnets[index].Id,
                Tags = Tags(environment),
            }, new CustomResourceOptions
            {
                DependsOn = { internetGateway },
            });
            var routeTable = new Aws.Ec2.RouteTable($"private-{index + 1}", new()
            {
                VpcId = vpc.Id,
                Routes = new[]
                {
                    new Aws.Ec2.Inputs.RouteTableRouteArgs
                    {
                        CidrBlock = "0.0.0.0/0",
                        NatGatewayId = gateway.Id,
                    },
                },
                Tags = Tags(environment),
            });
            var subnet = new Aws.Ec2.Subnet($"private-{index + 1}", new()
            {
                VpcId = vpc.Id,
                CidrBlock = $"10.42.{index + 10}.0/24",
                AvailabilityZone = availabilityZones.Apply(zones => zones.Names[index]),
                MapPublicIpOnLaunch = false,
                Tags = Tags(environment),
            });
            _ = new Aws.Ec2.RouteTableAssociation($"private-{index + 1}", new()
            {
                RouteTableId = routeTable.Id,
                SubnetId = subnet.Id,
            });
            return subnet;
        }).ToArray();
        var applicationSecurityGroup = new Aws.Ec2.SecurityGroup("application", new()
        {
            VpcId = vpc.Id,
            Egress = new[]
            {
                new Aws.Ec2.Inputs.SecurityGroupEgressArgs
                {
                    Protocol = "-1",
                    FromPort = 0,
                    ToPort = 0,
                    CidrBlocks = AllIpv4,
                },
            },
            Tags = Tags(environment),
        });
        var proxySecurityGroup = new Aws.Ec2.SecurityGroup("database-proxy", new()
        {
            VpcId = vpc.Id,
            Ingress = new[]
            {
                new Aws.Ec2.Inputs.SecurityGroupIngressArgs
                {
                    Protocol = "tcp",
                    FromPort = 5432,
                    ToPort = 5432,
                    SecurityGroups = new[] { applicationSecurityGroup.Id },
                },
            },
            Egress = new[]
            {
                new Aws.Ec2.Inputs.SecurityGroupEgressArgs
                {
                    Protocol = "tcp",
                    FromPort = 5432,
                    ToPort = 5432,
                    CidrBlocks = LicensingVpcIpv4,
                },
            },
            Tags = Tags(environment),
        });
        var databaseSecurityGroup = new Aws.Ec2.SecurityGroup("database", new()
        {
            VpcId = vpc.Id,
            Ingress = new[]
            {
                new Aws.Ec2.Inputs.SecurityGroupIngressArgs
                {
                    Protocol = "tcp",
                    FromPort = 5432,
                    ToPort = 5432,
                    SecurityGroups = new[] { proxySecurityGroup.Id },
                },
            },
            Tags = Tags(environment),
        });
        return new Network(
            vpc,
            publicSubnets,
            privateSubnets,
            applicationSecurityGroup,
            proxySecurityGroup,
            databaseSecurityGroup);
    }

    private static Aws.Rds.Instance CreateDatabase(
        string environment,
        Network network,
        Output<string> databasePassword)
    {
        var subnetGroup = new Aws.Rds.SubnetGroup("licensing", new()
        {
            SubnetIds = network.PrivateSubnets.Select(subnet => subnet.Id).ToArray(),
            Tags = Tags(environment),
        });
        return new Aws.Rds.Instance("licensing", new()
        {
            Engine = "postgres",
            EngineVersion = "17",
            InstanceClass = "db.t4g.micro",
            AllocatedStorage = 20,
            MaxAllocatedStorage = 100,
            StorageEncrypted = true,
            DbName = DatabaseName,
            Username = DatabaseMasterUsername,
            Password = databasePassword,
            DbSubnetGroupName = subnetGroup.Name,
            VpcSecurityGroupIds = new[] { network.DatabaseSecurityGroup.Id },
            BackupRetentionPeriod = 7,
            DeletionProtection = true,
            SkipFinalSnapshot = false,
            FinalSnapshotIdentifier = $"styrhous-licensing-{environment}-final",
            ApplyImmediately = false,
            Tags = Tags(environment),
        });
    }

    private static Queues CreateQueues(string environment)
    {
        var deadLetter = new Aws.Sqs.Queue("licensing-dead-letter", new()
        {
            MessageRetentionSeconds = 1_209_600,
            SqsManagedSseEnabled = true,
            Tags = Tags(environment),
        });
        var work = new Aws.Sqs.Queue("licensing-work", new()
        {
            VisibilityTimeoutSeconds = 900,
            MessageRetentionSeconds = 345_600,
            SqsManagedSseEnabled = true,
            RedrivePolicy = deadLetter.Arn.Apply(arn => JsonSerializer.Serialize(new
            {
                deadLetterTargetArn = arn,
                maxReceiveCount = 5,
            })),
            Tags = Tags(environment),
        });
        return new Queues(work, deadLetter);
    }

    private static Repositories CreateRepositories(string environment)
    {
        var api = new Aws.Ecr.Repository("licensing-api", new()
        {
            ImageScanningConfiguration = new Aws.Ecr.Inputs.RepositoryImageScanningConfigurationArgs
            {
                ScanOnPush = true,
            },
            ImageTagMutability = "IMMUTABLE",
            ForceDelete = false,
            Tags = Tags(environment),
        });
        var worker = new Aws.Ecr.Repository("licensing-worker", new()
        {
            ImageScanningConfiguration = new Aws.Ecr.Inputs.RepositoryImageScanningConfigurationArgs
            {
                ScanOnPush = true,
            },
            ImageTagMutability = "IMMUTABLE",
            ForceDelete = false,
            Tags = Tags(environment),
        });
        var apiLambdaPolicy = new Aws.Ecr.RepositoryPolicy("licensing-api-lambda", new()
        {
            Repository = api.Name,
            Policy = JsonSerializer.Serialize(new
            {
                Version = "2012-10-17",
                Statement = new[]
                {
                    new
                    {
                        Sid = "LambdaEcrImageRetrievalPolicy",
                        Effect = "Allow",
                        Principal = new { Service = "lambda.amazonaws.com" },
                        Action = EcrLambdaPullActions,
                    },
                },
            }),
        });
        var workerLambdaPolicy = new Aws.Ecr.RepositoryPolicy("licensing-worker-lambda", new()
        {
            Repository = worker.Name,
            Policy = JsonSerializer.Serialize(new
            {
                Version = "2012-10-17",
                Statement = new[]
                {
                    new
                    {
                        Sid = "LambdaEcrImageRetrievalPolicy",
                        Effect = "Allow",
                        Principal = new { Service = "lambda.amazonaws.com" },
                        Action = EcrLambdaPullActions,
                    },
                },
            }),
        });
        return new Repositories(api, worker, apiLambdaPolicy, workerLambdaPolicy);
    }

    private static Roles CreateRoles(string environment)
    {
        var lambdaTrust = TrustPolicy("lambda.amazonaws.com");
        var ecsTrust = TrustPolicy("ecs-tasks.amazonaws.com");
        var rdsTrust = TrustPolicy("rds.amazonaws.com");
        var api = new Aws.Iam.Role("licensing-api", new()
        {
            AssumeRolePolicy = lambdaTrust,
            Tags = Tags(environment),
        });
        var worker = new Aws.Iam.Role("licensing-worker", new()
        {
            AssumeRolePolicy = ecsTrust,
            Tags = Tags(environment),
        });
        var maintenance = new Aws.Iam.Role("licensing-maintenance", new()
        {
            AssumeRolePolicy = lambdaTrust,
            Tags = Tags(environment),
        });
        var migration = new Aws.Iam.Role("licensing-migration", new()
        {
            AssumeRolePolicy = ecsTrust,
            Tags = Tags(environment),
        });
        var execution = new Aws.Iam.Role("licensing-execution", new()
        {
            AssumeRolePolicy = ecsTrust,
            Tags = Tags(environment),
        });
        var proxy = new Aws.Iam.Role("licensing-db-proxy", new()
        {
            AssumeRolePolicy = rdsTrust,
            Tags = Tags(environment),
        });
        var apiVpcPolicy = new Aws.Iam.RolePolicyAttachment("api-vpc", new()
        {
            Role = api.Name,
            PolicyArn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole",
        });
        var maintenanceVpcPolicy = new Aws.Iam.RolePolicyAttachment("maintenance-vpc", new()
        {
            Role = maintenance.Name,
            PolicyArn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole",
        });
        var executionPolicy = new Aws.Iam.RolePolicyAttachment("execution", new()
        {
            Role = execution.Name,
            PolicyArn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy",
        });
        return new Roles(
            api,
            worker,
            maintenance,
            migration,
            execution,
            proxy,
            apiVpcPolicy,
            maintenanceVpcPolicy,
            executionPolicy);
    }

    private static Logs CreateLogs(string environment)
    {
        return new(
        new Aws.CloudWatch.LogGroup("licensing-api", new()
        {
            RetentionInDays = 30,
            Tags = Tags(environment),
        }),
        new Aws.CloudWatch.LogGroup("licensing-worker", new()
        {
            RetentionInDays = 30,
            Tags = Tags(environment),
        }),
        new Aws.CloudWatch.LogGroup("licensing-maintenance", new()
        {
            RetentionInDays = 30,
            Tags = Tags(environment),
        }));
    }

    private static ApplicationSecret CreateSecret(
        string name,
        string environment,
        Output<string> contents)
    {
        var secret = new Aws.SecretsManager.Secret(name, new()
        {
            RecoveryWindowInDays = 30,
            Tags = Tags(environment),
        });
        var version = new Aws.SecretsManager.SecretVersion(name, new()
        {
            SecretId = secret.Id,
            SecretString = contents,
        });
        return new ApplicationSecret(secret, version);
    }

    private static Aws.Rds.Proxy CreateDatabaseProxy(
        string environment,
        Network network,
        Aws.Rds.Instance database,
        ApplicationSecret databaseMasterCredentials,
        ApplicationSecret databaseRuntimeCredentials,
        Aws.Iam.Role role)
    {
        var secretPolicy = new Aws.Iam.RolePolicy("proxy-secret", new()
        {
            Role = role.Name,
            Policy = Output.Tuple(
                databaseMasterCredentials.Secret.Arn,
                databaseRuntimeCredentials.Secret.Arn).Apply(arns =>
                    AccessPolicy(SecretReadActions, new[] { arns.Item1, arns.Item2 })),
        });
        var proxy = new Aws.Rds.Proxy("licensing", new()
        {
            EngineFamily = "POSTGRESQL",
            RoleArn = role.Arn,
            VpcSubnetIds = network.PrivateSubnets.Select(subnet => subnet.Id).ToArray(),
            VpcSecurityGroupIds = new[] { network.ProxySecurityGroup.Id },
            RequireTls = true,
            Auths = new[]
            {
                new Aws.Rds.Inputs.ProxyAuthArgs
                {
                    AuthScheme = "SECRETS",
                    IamAuth = "DISABLED",
                    SecretArn = databaseMasterCredentials.Secret.Arn,
                },
                new Aws.Rds.Inputs.ProxyAuthArgs
                {
                    AuthScheme = "SECRETS",
                    IamAuth = "DISABLED",
                    SecretArn = databaseRuntimeCredentials.Secret.Arn,
                },
            },
            Tags = Tags(environment),
        }, new CustomResourceOptions
        {
            DependsOn =
            {
                databaseMasterCredentials.Version,
                databaseRuntimeCredentials.Version,
                secretPolicy,
            },
        });
        var group = new Aws.Rds.ProxyDefaultTargetGroup("licensing", new()
        {
            DbProxyName = proxy.Name,
            ConnectionPoolConfig = new Aws.Rds.Inputs.ProxyDefaultTargetGroupConnectionPoolConfigArgs
            {
                MaxConnectionsPercent = 90,
                MaxIdleConnectionsPercent = 20,
                ConnectionBorrowTimeout = 30,
            },
        });
        _ = new Aws.Rds.ProxyTarget("licensing", new()
        {
            DbProxyName = proxy.Name,
            TargetGroupName = group.Name,
            DbInstanceIdentifier = database.Identifier,
        });
        return proxy;
    }

}
