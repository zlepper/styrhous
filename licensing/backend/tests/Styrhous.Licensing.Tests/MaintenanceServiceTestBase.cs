using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests;

internal static class MaintenanceServiceTestBase
{
    public static ServiceTestBase<T> ForDatabase<T>(PostgresTestDatabase database,
        DateTimeOffset observedAt, string queueName) where T : notnull
    {
        var host = Program.BuildMaintenanceHost([
            "--environment=Production",
            $"--ConnectionStrings:Licensing={database.ConnectionString}",
            "--Messaging:Transport=RabbitMq",
            $"--Messaging:QueueName={queueName}",
            $"--Messaging:RabbitMq:ConnectionString={RabbitMqTestConnection.Value}",
        ], applicationConfiguration: null, builder =>
        {
            builder.ConfigureContainer(new DefaultServiceProviderFactory(new ServiceProviderOptions
            {
                ValidateScopes = true,
                ValidateOnBuild = true,
            }));
            builder.Services.Replace(ServiceDescriptor.Singleton<TimeProvider>(new FixedTimeProvider(observedAt)));
        });
        return ServiceTestBase<T>.FromHost(database, host);
    }
}
