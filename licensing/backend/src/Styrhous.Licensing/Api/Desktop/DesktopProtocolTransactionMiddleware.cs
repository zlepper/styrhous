using System.Data;
using Microsoft.AspNetCore.Http.Features;
using Microsoft.EntityFrameworkCore;
using OpenIddict.Abstractions;
using OpenIddict.EntityFrameworkCore.Models;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Api.Desktop;

public sealed class DesktopProtocolTransactionMiddleware(RequestDelegate next)
{
    private const int MaximumRequestBodySize = 16 * 1024;

    public async Task InvokeAsync(
        HttpContext httpContext,
        DesktopProtocolTransaction transaction,
        LicensingDbContext dbContext,
        IDbContextFactory<LicensingDbContext> dbContextFactory,
        TimeProvider timeProvider)
    {
        if (!HttpMethods.IsPost(httpContext.Request.Method)
            || (httpContext.Request.Path != DesktopProtocolConstants.DeviceAuthorizationPath
                && httpContext.Request.Path != DesktopProtocolConstants.ApprovalPath
                && httpContext.Request.Path != DesktopProtocolConstants.TokenPath
                && httpContext.Request.Path != DesktopProtocolConstants.RevocationPath))
        {
            await next(httpContext);
            return;
        }

        if (httpContext.Request.ContentLength > MaximumRequestBodySize)
        {
            httpContext.Response.StatusCode = StatusCodes.Status413PayloadTooLarge;
            return;
        }

        var requestSizeFeature =
            httpContext.Features.Get<IHttpMaxRequestBodySizeFeature>();
        if (requestSizeFeature is { IsReadOnly: false })
        {
            requestSizeFeature.MaxRequestBodySize = MaximumRequestBodySize;
        }

        if (httpContext.Request.HasFormContentType)
        {
            httpContext.Request.EnableBuffering(
                bufferThreshold: MaximumRequestBodySize,
                bufferLimit: MaximumRequestBodySize);
            try
            {
                await httpContext.Request.ReadFormAsync(httpContext.RequestAborted);
            }
            catch (InvalidDataException)
            {
                await WriteInvalidRequestAsync(httpContext);
                return;
            }
            catch (IOException)
            {
                httpContext.Response.StatusCode = StatusCodes.Status413PayloadTooLarge;
                return;
            }
        }

        if (httpContext.Request.Path == DesktopProtocolConstants.DeviceAuthorizationPath)
        {
            await next(httpContext);
            return;
        }

        var originalResponseBodyFeature =
            httpContext.Features.Get<IHttpResponseBodyFeature>()
            ?? throw new InvalidOperationException(
                "The HTTP response body feature is unavailable.");
        await using var responseBuffer = new MemoryStream();
        var bufferingResponseBodyFeature = new StreamResponseBodyFeature(responseBuffer);
        httpContext.Features.Set<IHttpResponseBodyFeature>(bufferingResponseBodyFeature);
        var releaseResponse = false;
        try
        {
            await transaction.ExecuteAsync(async () =>
            {
                DesktopProtocolRequestState.Clear(httpContext);
                responseBuffer.SetLength(0);
                httpContext.Response.Clear();
                await next(httpContext);

                var lostAuthorizationDecision =
                    httpContext.Request.Path == DesktopProtocolConstants.ApprovalPath
                    && DesktopProtocolRequestState.TryGetAuthorizationDecision(
                        httpContext,
                        out var decisionOperation)
                    && (httpContext.Response.StatusCode is >= 400 and < 500
                        || !await IsAuthorizationDecisionAppliedAsync(
                            dbContext,
                            decisionOperation,
                            httpContext.RequestAborted));
                if (lostAuthorizationDecision)
                {
                    await transaction.RollbackAsync(CancellationToken.None);
                    await WriteConcurrentModificationAsync(httpContext);
                }
                else if (transaction.IsRetryableFailure)
                {
                    // OpenIddict may already have redeemed the submitted token. Undo
                    // that write as well so the same credential remains retryable.
                    await transaction.RollbackAsync(CancellationToken.None);
                }
                else if (httpContext.Request.Path == DesktopProtocolConstants.TokenPath
                    || httpContext.Response.StatusCode is >= 200 and < 300)
                {
                    await transaction.CommitAsync(httpContext.RequestAborted);
                }
                else
                {
                    await transaction.RollbackAsync(CancellationToken.None);
                }

                releaseResponse = true;
            }, httpContext.RequestAborted);
        }
        catch (Exception exception) when (
            httpContext.Request.Path == DesktopProtocolConstants.TokenPath
            && IsConcurrencyLoss(exception)
            && DesktopProtocolRequestState.TryGet(httpContext, out var operation)
            && operation.TokenGrant is not null)
        {
            await transaction.RollbackAsync(CancellationToken.None);
            var retryable = false;
            if (operation.TokenGrant is DesktopTokenGrant.RefreshToken)
            {
                retryable = await RecoverConcurrentAuthorizationAsync(
                    dbContextFactory,
                    operation,
                    timeProvider.GetUtcNow());
            }

            if (retryable)
            {
                await WriteRetryableGrantAsync(httpContext);
            }
            else
            {
                await WriteInvalidGrantAsync(httpContext);
            }
            releaseResponse = true;
        }
        catch (Exception exception) when (
            httpContext.Request.Path == DesktopProtocolConstants.RevocationPath
            && IsConcurrencyLoss(exception)
            && DesktopProtocolRequestState.TryGet(httpContext, out var operation)
            && operation.TokenGrant is null)
        {
            await transaction.RollbackAsync(CancellationToken.None);
            await RecoverConcurrentAuthorizationAsync(
                dbContextFactory,
                operation,
                timeProvider.GetUtcNow());
            httpContext.Response.Clear();
            httpContext.Response.StatusCode = StatusCodes.Status200OK;
            httpContext.Response.Headers.CacheControl = "no-store";
            releaseResponse = true;
        }
        catch (Exception exception) when (
            httpContext.Request.Path == DesktopProtocolConstants.ApprovalPath
            && IsConcurrencyLoss(exception))
        {
            await transaction.RollbackAsync(CancellationToken.None);
            await WriteConcurrentModificationAsync(httpContext);
            releaseResponse = true;
        }
        catch
        {
            await transaction.RollbackAsync(CancellationToken.None);
            throw;
        }
        finally
        {
            try
            {
                if (releaseResponse)
                {
                    await bufferingResponseBodyFeature.CompleteAsync();
                }
            }
            finally
            {
                httpContext.Features.Set(originalResponseBodyFeature);
                await transaction.DisposeAsync();
            }
        }

        if (releaseResponse)
        {
            responseBuffer.Position = 0;
            await responseBuffer.CopyToAsync(
                originalResponseBodyFeature.Stream,
                CancellationToken.None);
        }
    }

    private static async Task<bool> RecoverConcurrentAuthorizationAsync(
        IDbContextFactory<LicensingDbContext> dbContextFactory,
        DesktopProtocolOperation operation,
        DateTimeOffset observedAt)
    {
        return await EfConcurrencyRetry.ExecuteAsync(
            () => LicensingDbContextTransaction.ExecuteAsync(
                dbContextFactory,
                IsolationLevel.Serializable,
                async (recoveryContext, cancellationToken) =>
                {
                    await EfTransactionSerialization.TryClaimUserAsync(
                        recoveryContext,
                        operation.UserId,
                        cancellationToken);
                    // A serializable conflict can come from billing renewal, not
                    // just refresh-token reuse. Classify the committed credential
                    // under the same user lock used by issuance and revocation.
                    var subject = operation.UserId.ToString();
                    if (operation.TokenGrant is DesktopTokenGrant.RefreshToken
                        && await recoveryContext.Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
                            .AnyAsync(token => token.Id == operation.TokenId
                                && token.Subject == subject
                                && token.Type == OpenIddictConstants.TokenTypeIdentifiers.RefreshToken
                                && token.Status == OpenIddictConstants.Statuses.Valid
                                && token.ExpirationDate > observedAt.UtcDateTime
                                && token.Authorization != null
                                && token.Authorization.Id == operation.AuthorizationId
                                && token.Authorization.Status == OpenIddictConstants.Statuses.Valid,
                                cancellationToken)
                        && await recoveryContext.DesktopDeviceSessions.AnyAsync(
                            session => session.AuthorizationId == operation.AuthorizationId
                                && session.RevokedAt == null,
                            cancellationToken))
                    {
                        return true;
                    }
                    await DesktopSessionRevocation.RevokeDesktopAuthorizationsAsync(
                        recoveryContext,
                        [operation.AuthorizationId],
                        observedAt,
                        cancellationToken);
                    await recoveryContext.SaveChangesAsync(cancellationToken);
                    return false;
                },
                CancellationToken.None));
    }

    private static Task<bool> IsAuthorizationDecisionAppliedAsync(
        LicensingDbContext dbContext,
        DesktopAuthorizationDecisionOperation operation,
        CancellationToken cancellationToken)
    {
        var expectedStatus = operation.Decision switch
        {
            DesktopAuthorizationDecision.Approve => OpenIddictConstants.Statuses.Redeemed,
            DesktopAuthorizationDecision.Deny => OpenIddictConstants.Statuses.Rejected,
            _ => throw new InvalidOperationException(
                $"Unsupported desktop authorization decision: {operation.Decision}."),
        };
        return dbContext.Set<OpenIddictEntityFrameworkCoreToken<Guid>>()
            .AsNoTracking()
            .AnyAsync(
                token => token.Id == operation.TokenId
                    && token.Status == expectedStatus,
                cancellationToken);
    }

    private static async Task WriteRetryableGrantAsync(HttpContext httpContext)
    {
        httpContext.Response.Clear();
        httpContext.Response.StatusCode = StatusCodes.Status503ServiceUnavailable;
        httpContext.Response.Headers.CacheControl = "no-store";
        httpContext.Response.Headers.Pragma = "no-cache";
        await httpContext.Response.WriteAsJsonAsync(new
        {
            error = OpenIddictConstants.Errors.TemporarilyUnavailable,
            error_description = "The license changed during refresh. Retry with the same credential.",
        }, CancellationToken.None);
    }

    private static async Task WriteInvalidGrantAsync(HttpContext httpContext)
    {
        httpContext.Response.Clear();
        httpContext.Response.StatusCode = StatusCodes.Status400BadRequest;
        httpContext.Response.ContentType = "application/json; charset=utf-8";
        httpContext.Response.Headers.CacheControl = "no-store";
        httpContext.Response.Headers.Pragma = "no-cache";
        await httpContext.Response.WriteAsJsonAsync(
            new Dictionary<string, string>(StringComparer.Ordinal)
            {
                ["error"] = "invalid_grant",
                ["error_description"] =
                    "The desktop authorization changed while the token was issued.",
            },
            CancellationToken.None);
    }

    private static async Task WriteInvalidRequestAsync(HttpContext httpContext)
    {
        httpContext.Response.StatusCode = StatusCodes.Status400BadRequest;
        httpContext.Response.ContentType = "application/json; charset=utf-8";
        httpContext.Response.Headers.CacheControl = "no-store";
        httpContext.Response.Headers.Pragma = "no-cache";
        await httpContext.Response.WriteAsJsonAsync(
            new Dictionary<string, string>(StringComparer.Ordinal)
            {
                ["error"] = "invalid_request",
                ["error_description"] = "The desktop protocol form is malformed.",
            },
            CancellationToken.None);
    }

    private static async Task WriteConcurrentModificationAsync(
        HttpContext httpContext)
    {
        httpContext.Response.Clear();
        httpContext.Response.StatusCode = StatusCodes.Status409Conflict;
        httpContext.Response.ContentType = "application/json; charset=utf-8";
        httpContext.Response.Headers.CacheControl = "no-store";
        await httpContext.Response.WriteAsJsonAsync(
            DesktopDeviceAuthorizationErrorResponse.ConcurrentModification(),
            CancellationToken.None);
    }

    private static bool IsConcurrencyLoss(Exception exception)
    {
        return EfConcurrencyFailure.IsRetryable(exception);
    }
}
