using System.Buffers;
using System.Text;
using Microsoft.AspNetCore.Http.HttpResults;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Operations;

namespace Styrhous.Licensing.Api.Billing;

public static class BillingWebhookEndpoints
{
    public const string SignatureHeaderName = "Stripe-Signature";

    internal const int MaximumPayloadSizeBytes = 256 * 1024;

    public static IEndpointRouteBuilder MapBillingWebhookEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapPost("/api/webhooks/stripe", ReceiveAsync);
        return endpoints;
    }

    private static async Task<IResult> ReceiveAsync(
        HttpRequest request,
        BillingWebhookIngestionService service,
        ILoggerFactory loggerFactory,
        CancellationToken cancellationToken)
    {
        var logger = loggerFactory.CreateLogger(
            "Styrhous.Licensing.Api.Billing.BillingWebhook");
        var body = await ReadBodyAsync(request, cancellationToken);
        if (body.Status == WebhookBodyStatus.TooLarge)
        {
            LogWebhookRejected(logger, BillingWebhookReasonCodes.PayloadTooLarge, null);
            return BillingError(
                StatusCodes.Status413PayloadTooLarge,
                BillingWebhookReasonCodes.PayloadTooLarge);
        }

        if (body.Status == WebhookBodyStatus.InvalidEncoding)
        {
            LogWebhookRejected(logger, BillingWebhookReasonCodes.InvalidPayload, null);
            return BillingError(
                StatusCodes.Status400BadRequest,
                BillingWebhookReasonCodes.InvalidPayload);
        }

        var signature = request.Headers[SignatureHeaderName].ToString();
        BillingWebhookIngestionResult result;
        try
        {
            result = await service.IngestAsync(
                body.Payload!,
                signature,
                cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            LogWebhookFailure(logger, "processing_exception", exception);
            throw;
        }

        return result.Status switch
        {
            BillingWebhookIngestionStatus.Received =>
                TypedResults.Ok(
                    new BillingWebhookResponse(
                        BillingWebhookReasonCodes.Received,
                        result.InboxEventId)),
            BillingWebhookIngestionStatus.Duplicate =>
                TypedResults.Ok(
                    new BillingWebhookResponse(
                        BillingWebhookReasonCodes.Duplicate,
                        result.InboxEventId)),
            BillingWebhookIngestionStatus.Ignored =>
                TypedResults.Ok(
                    new BillingWebhookResponse(
                        BillingWebhookReasonCodes.Ignored,
                        result.InboxEventId)),
            BillingWebhookIngestionStatus.InvalidSignature =>
                LoggedRejectedBillingError(
                    logger,
                    BillingWebhookReasonCodes.InvalidSignature),
            BillingWebhookIngestionStatus.InvalidPayload =>
                LoggedFailedBillingError(
                    logger,
                    BillingWebhookReasonCodes.InvalidPayload),
            _ => throw new InvalidOperationException(
                $"Unsupported billing webhook ingestion status: {result.Status}."),
        };
    }

    private static JsonHttpResult<BillingWebhookResponse> LoggedRejectedBillingError(
        ILogger logger,
        string reasonCode)
    {
        LogWebhookRejected(logger, reasonCode, null);
        return BillingError(StatusCodes.Status400BadRequest, reasonCode);
    }

    private static JsonHttpResult<BillingWebhookResponse> LoggedFailedBillingError(
        ILogger logger,
        string reasonCode)
    {
        LogWebhookFailure(logger, reasonCode, null);
        return BillingError(StatusCodes.Status400BadRequest, reasonCode);
    }

    private static readonly Action<ILogger, string, Exception?> LogWebhookFailure =
        LoggerMessage.Define<string>(
            LogLevel.Warning,
            new EventId(
                LicensingOperationalMetrics.StripeWebhookFailureEventId,
                "StripeWebhookFailure"),
            "Stripe webhook request failed: {FailureReason}.");

    private static readonly Action<ILogger, string, Exception?> LogWebhookRejected =
        LoggerMessage.Define<string>(
            LogLevel.Warning,
            new EventId(
                LicensingOperationalMetrics.StripeWebhookRejectedEventId,
                "StripeWebhookRejected"),
            "Stripe webhook request rejected: {FailureReason}.");

    private static JsonHttpResult<BillingWebhookResponse> BillingError(
        int statusCode,
        string reasonCode)
    {
        return TypedResults.Json(
            new BillingWebhookResponse(reasonCode, InboxEventId: null),
            statusCode: statusCode);
    }

    private static async Task<WebhookBody> ReadBodyAsync(
        HttpRequest request,
        CancellationToken cancellationToken)
    {
        if (request.ContentLength > MaximumPayloadSizeBytes)
        {
            return WebhookBody.TooLarge();
        }

        var rentedBuffer = ArrayPool<byte>.Shared.Rent(16 * 1024);
        try
        {
            using var body = new MemoryStream();
            while (true)
            {
                var bytesRead = await request.Body.ReadAsync(
                    rentedBuffer.AsMemory(),
                    cancellationToken);
                if (bytesRead == 0)
                {
                    break;
                }

                if (body.Length + bytesRead > MaximumPayloadSizeBytes)
                {
                    return WebhookBody.TooLarge();
                }

                await body.WriteAsync(
                    rentedBuffer.AsMemory(0, bytesRead),
                    cancellationToken);
            }

            try
            {
                return WebhookBody.Valid(
                    new UTF8Encoding(
                        encoderShouldEmitUTF8Identifier: false,
                        throwOnInvalidBytes: true)
                        .GetString(body.GetBuffer(), 0, checked((int)body.Length)));
            }
            catch (DecoderFallbackException)
            {
                return WebhookBody.InvalidEncoding();
            }
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(rentedBuffer);
        }
    }

    private enum WebhookBodyStatus
    {
        Valid,
        TooLarge,
        InvalidEncoding,
    }

    private sealed record WebhookBody(WebhookBodyStatus Status, string? Payload)
    {
        public static WebhookBody Valid(string payload)
        {
            return new(WebhookBodyStatus.Valid, payload);
        }

        public static WebhookBody TooLarge()
        {
            return new(WebhookBodyStatus.TooLarge, Payload: null);
        }

        public static WebhookBody InvalidEncoding()
        {
            return new(WebhookBodyStatus.InvalidEncoding, Payload: null);
        }
    }
}
