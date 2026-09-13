using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;

namespace Styrhous.Licensing.Tests;

internal sealed class TestLogging : IDisposable
{
    private readonly ServiceProvider _services = new ServiceCollection()
        .AddLogging(Program.ConfigureStructuredLogging)
        .BuildServiceProvider(new ServiceProviderOptions
        {
            ValidateScopes = true,
            ValidateOnBuild = true,
        });

    public ILogger<T> GetLogger<T>()
    {
        return _services.GetRequiredService<ILogger<T>>();
    }

    public void Dispose()
    {
        _services.Dispose();
    }
}
