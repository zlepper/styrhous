using Microsoft.Extensions.Hosting;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.AspNetCore.TestHost;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Configuration;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Styrhous.Licensing.Tests.Api;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests;

internal sealed class ServiceTestBase<T> : IAsyncDisposable where T : notnull
{
    private readonly IServiceProvider _rootServices;
    private readonly Func<ValueTask> _disposeHost;
    private readonly AsyncServiceScope _scope;
    private readonly bool _ownsDatabase;

    private ServiceTestBase(PostgresTestDatabase database, IServiceProvider rootServices,
        Func<ValueTask> disposeHost, bool ownsDatabase)
    {
        Database = database;
        _rootServices = rootServices;
        _disposeHost = disposeHost;
        _ownsDatabase = ownsDatabase;
        _scope = rootServices.CreateAsyncScope();
        try
        {
            Service = _scope.ServiceProvider.GetRequiredService<T>();
        }
        catch
        {
            _scope.Dispose();
            throw;
        }
    }

    private static ServiceTestBase<T> CreateApplicationFixture(
        PostgresTestDatabase database, DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices, bool ownsDatabase,
        Action<IConfiguration, IServiceCollection>? configureHostServices, IInterceptor[] interceptors)
    {
        var application = new LicensingWebApplicationFactory(database, observedAt, interceptors: interceptors);
        var configuredApplication = application.WithWebHostBuilder(builder =>
        {
            builder.UseDefaultServiceProvider(options =>
            {
                options.ValidateScopes = true;
                options.ValidateOnBuild = true;
            });
            if (configureHostServices is not null)
            {
                builder.ConfigureServices((context, services) =>
                    configureHostServices(context.Configuration, services));
            }
            if (configureServices is not null)
            {
                builder.ConfigureTestServices(configureServices);
            }
        });
        try
        {
            return new ServiceTestBase<T>(database, configuredApplication.Services, async () =>
            {
                try
                {
                    await configuredApplication.DisposeAsync();
                }
                finally
                {
                    await application.DisposeAsync();
                }
            }, ownsDatabase);
        }
        catch
        {
            try
            {
                configuredApplication.Dispose();
            }
            finally
            {
                application.Dispose();
            }
            throw;
        }
    }

    public static ServiceTestBase<T> FromHost(PostgresTestDatabase database, IHost host, bool ownsDatabase = false)
    {
        try
        {
            return new ServiceTestBase<T>(database, host.Services, async () =>
            {
                if (host is IAsyncDisposable asyncDisposable)
                {
                    await asyncDisposable.DisposeAsync();
                }
                else
                {
                    host.Dispose();
                }
            }, ownsDatabase);
        }
        catch
        {
            host.Dispose();
            throw;
        }
    }

    public PostgresTestDatabase Database { get; }
    public T Service { get; }
    public IServiceProvider Services => _scope.ServiceProvider;

    public AsyncServiceScope CreateScope()
    {
        return _rootServices.CreateAsyncScope();
    }

    public static async Task<ServiceTestBase<T>> CreateAsync(
        DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices = null,
        Action<IConfiguration, IServiceCollection>? configureHostServices = null,
        params IInterceptor[] interceptors)
    {
        var database = await PostgresTestDatabase.CreateAsync();
        try
        {
            return CreateApplicationFixture(database, observedAt, configureServices, true, configureHostServices, interceptors);
        }
        catch
        {
            await database.DisposeAsync();
            throw;
        }
    }

    public static ServiceTestBase<T> ForDatabase(
        PostgresTestDatabase database,
        DateTimeOffset observedAt,
        Action<IServiceCollection>? configureServices = null,
        Action<IConfiguration, IServiceCollection>? configureHostServices = null,
        params IInterceptor[] interceptors)
    {
        return CreateApplicationFixture(database, observedAt, configureServices, false, configureHostServices, interceptors);
    }

    public async ValueTask DisposeAsync()
    {
        try
        {
            await _scope.DisposeAsync();
        }
        finally
        {
            try
            {
                await _disposeHost();
            }
            finally
            {
                if (_ownsDatabase)
                {
                    await Database.DisposeAsync();
                }
            }
        }
    }
}
