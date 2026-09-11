using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Runtime;

internal sealed class LicensingStartupValidation(
    IServiceScopeFactory scopeFactory,
    IHostEnvironment environment) : IHostedService
{

    public async Task StartAsync(CancellationToken cancellationToken)
    {
        if (environment.IsEnvironment("Testing"))
        {
            return;
        }

        await using var scope = scopeFactory.CreateAsyncScope();
        var context = scope.ServiceProvider.GetRequiredService<LicensingDbContext>();
        var pending = await context.Database.GetPendingMigrationsAsync(cancellationToken);
        var migration = pending.FirstOrDefault();
        if (migration is not null)
        {
            throw new InvalidOperationException(
                $"The licensing database schema is not current. Run the migrate command; "
                    + $"pending migration: {migration}.");
        }
    }

    public Task StopAsync(CancellationToken cancellationToken)
    {
        return Task.CompletedTask;
    }

}
