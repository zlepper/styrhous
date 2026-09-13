using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Http;

namespace Styrhous.Licensing.Tests.Api;

internal sealed class RequestCompletionObserver(string method, PathString path) : IStartupFilter
{
    private readonly TaskCompletionSource _requestCompleted =
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    public bool WasCanceled { get; private set; }

    public Action<IApplicationBuilder> Configure(Action<IApplicationBuilder> next)
    {
        return application =>
        {
            application.Use(async (context, nextMiddleware) =>
            {
                if (!string.Equals(
                        context.Request.Method,
                        method,
                        StringComparison.OrdinalIgnoreCase)
                    || context.Request.Path != path)
                {
                    await nextMiddleware();
                    return;
                }

                try
                {
                    await nextMiddleware();
                }
                catch (OperationCanceledException)
                {
                    WasCanceled = true;
                    throw;
                }
                finally
                {
                    _requestCompleted.TrySetResult();
                }
            });
            next(application);
        };
    }

    public async Task WaitUntilCompletedAsync(CancellationToken cancellationToken = default)
    {
        try
        {
            await _requestCompleted.Task.WaitAsync(
                TimeSpan.FromSeconds(10),
                cancellationToken);
        }
        catch (TimeoutException exception)
        {
            throw new TimeoutException(
                "The expected request did not complete on the server.",
                exception);
        }
    }
}
