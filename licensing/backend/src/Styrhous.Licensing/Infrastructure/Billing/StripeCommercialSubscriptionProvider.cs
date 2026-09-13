using Styrhous.Licensing.Persistence;
using System.Net;
using Microsoft.Extensions.Options;
using Stripe;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Infrastructure.Billing;

public sealed partial class StripeCommercialSubscriptionProvider(
    IStripeClient stripeClient,
    IOptions<StripeBillingOptions> options,
    PostgresBillingProviderReadRevisionSource providerReadRevisions,
    ILogger<StripeCommercialSubscriptionProvider> logger)
    : ICommercialSubscriptionProvider, IBillingSeatQuantityProvider
{
    private readonly EventService _events = new(stripeClient);
    private readonly SubscriptionService _subscriptions = new(stripeClient);
    private readonly HashSet<string> _allowedPriceIds = CreateAllowedPriceIds(options.Value);

    public Task<BillingSeatQuantityProviderResult> ObserveAsync(
        BillingSeatQuantityProviderRequest request,
        CancellationToken cancellationToken)
    {
        return ResolveSeatQuantityAsync(
            request,
            mutationAuthorized: false,
            automaticReplayEndsAt: null,
            cancellationToken);
    }

    public Task<BillingSeatQuantityProviderResult> ApplyAsync(
        BillingSeatQuantityProviderRequest request,
        DateTimeOffset automaticReplayEndsAt,
        CancellationToken cancellationToken)
    {
        return ResolveSeatQuantityAsync(
            request,
            mutationAuthorized: true,
            automaticReplayEndsAt,
            cancellationToken);
    }

    private async Task<BillingSeatQuantityProviderResult> ResolveSeatQuantityAsync(
        BillingSeatQuantityProviderRequest request,
        bool mutationAuthorized,
        DateTimeOffset? automaticReplayEndsAt,
        CancellationToken cancellationToken)
    {
        ValidateSeatQuantityRequest(request);
        if (mutationAuthorized && automaticReplayEndsAt == default)
        {
            throw new ArgumentOutOfRangeException(nameof(automaticReplayEndsAt));
        }

        var mutationRequested = false;
        try
        {
            var subscription = await _subscriptions.GetAsync(
                request.ExternalSubscriptionId,
                options: null,
                requestOptions: null,
                cancellationToken);
            var providerObservedAt = RequireProviderObservedAt(subscription);
            var providerReadRevision = await providerReadRevisions.ReserveAsync(
                cancellationToken);
            var authoritative = ResolveSubscription(
                subscription,
                ignoreUnmanagedSubscription: false,
                expectedBillingOperationId: null,
                providerObservedAt,
                providerReadRevision)
                ?? throw new InvalidOperationException(
                    "The Stripe subscription is not managed by Styrhous.");
            EnsureSeatQuantitySubscription(request, subscription, authoritative);
            if (!CommercialEntitlement.Resolve(
                    authoritative.Projection.Status,
                    authoritative.Projection.CancelAtPeriodEnd,
                    authoritative.Projection.CurrentPeriodStartedAt,
                    authoritative.Projection.CurrentPeriodEndsAt,
                    providerObservedAt)
                .IsEligible)
            {
                return new BillingSeatQuantityProviderResult(
                    BillingSeatQuantityProviderStatus.Superseded,
                    authoritative);
            }

            var currentSeatQuantity = authoritative.Projection.SeatQuantity;
            if (currentSeatQuantity == request.SeatQuantity)
            {
                return new BillingSeatQuantityProviderResult(
                    BillingSeatQuantityProviderStatus.Applied,
                    authoritative);
            }

            if (currentSeatQuantity != request.PreviousSeatQuantity)
            {
                return new BillingSeatQuantityProviderResult(
                    BillingSeatQuantityProviderStatus.Superseded,
                    authoritative);
            }

            if (!mutationAuthorized
                || providerObservedAt >= automaticReplayEndsAt!.Value.ToUniversalTime())
            {
                return new BillingSeatQuantityProviderResult(
                    BillingSeatQuantityProviderStatus.MutationRequired,
                    authoritative);
            }

            var item = ResolveSingleSubscriptionItem(subscription);
            mutationRequested = true;
            var updated = await _subscriptions.UpdateAsync(
                request.ExternalSubscriptionId,
                new SubscriptionUpdateOptions
                {
                    Items =
                    [
                        new SubscriptionItemOptions
                        {
                            Id = RequiredProviderValue(item.Id, "subscription item"),
                            Quantity = request.SeatQuantity,
                        },
                    ],
                    PaymentBehavior = request.SeatQuantity > currentSeatQuantity
                        ? "allow_incomplete"
                        : null,
                    ProrationBehavior = request.SeatQuantity > currentSeatQuantity
                        ? "always_invoice"
                        : "none",
                },
                new RequestOptions
                {
                    IdempotencyKey = $"styrhous-seat-quantity-{request.BillingOperationId:N}",
                },
                cancellationToken);
            var updatedProviderObservedAt = RequireProviderObservedAt(updated);
            var updatedProviderReadRevision = await providerReadRevisions.ReserveAsync(
                cancellationToken);
            var updatedAuthoritative = ResolveSubscription(
                updated,
                ignoreUnmanagedSubscription: false,
                expectedBillingOperationId: null,
                updatedProviderObservedAt,
                updatedProviderReadRevision,
                CommercialSubscriptionSnapshotKind.MutationResponse)
                ?? throw new InvalidOperationException(
                    "The updated Stripe subscription is not managed by Styrhous.");
            EnsureSeatQuantitySubscription(request, updated, updatedAuthoritative);
            if (updatedAuthoritative.Projection.SeatQuantity != request.SeatQuantity)
            {
                throw new InvalidOperationException(
                    "Stripe did not apply the requested seat quantity.");
            }

            return new BillingSeatQuantityProviderResult(
                BillingSeatQuantityProviderStatus.Applied,
                updatedAuthoritative);
        }
        catch (TaskCanceledException exception) when (!cancellationToken.IsCancellationRequested)
        {
            LogTransientProviderFailure(logger, "timeout");
            throw SeatQuantityProviderUnavailable(exception);
        }
        catch (HttpRequestException exception)
        {
            LogTransientProviderFailure(logger, "network");
            throw SeatQuantityProviderUnavailable(exception);
        }
        catch (StripeException exception) when (
            StripeBillingFailureClassifier.IsTransient(exception.HttpStatusCode))
        {
            LogTransientStripeFailure(
                logger,
                exception.HttpStatusCode,
                exception.StripeError?.Type,
                exception.StripeError?.Code,
                exception.StripeResponse?.RequestId);
            if (mutationRequested
                && StripeBillingFailureClassifier.IsCachedServerFailure(
                    exception.HttpStatusCode))
            {
                throw new BillingSeatQuantityProviderIndeterminateException(
                    "Stripe cached an indeterminate seat-quantity update result.",
                    exception);
            }

            throw SeatQuantityProviderUnavailable(exception);
        }
        catch (StripeException exception)
        {
            LogPermanentStripeRejection(
                logger,
                exception.HttpStatusCode,
                exception.StripeError?.Type,
                exception.StripeError?.Code,
                exception.StripeResponse?.RequestId);
            throw new BillingSeatQuantityProviderRejectedException(
                "Stripe rejected the seat-quantity change.");
        }
    }

    public async Task<AuthoritativeCommercialSubscription?> ResolveEventAsync(
        string externalEventId,
        BillingWebhookEventKind kind,
        CancellationToken cancellationToken)
    {
        var providerEvent = await _events.GetAsync(
            externalEventId,
            options: null,
            requestOptions: null,
            cancellationToken);
        if (!string.Equals(providerEvent.Id, externalEventId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Stripe returned a different event than the one requested.");
        }

        var providerKind = StripeBillingWebhookEventTypes.ToKind(providerEvent.Type);
        if (providerKind != kind || providerKind == BillingWebhookEventKind.Unsupported)
        {
            throw new InvalidOperationException(
                "The persisted billing event kind does not match Stripe's event.");
        }

        var externalSubscriptionId = ResolveSubscriptionId(providerEvent, kind);
        if (string.IsNullOrWhiteSpace(externalSubscriptionId))
        {
            return null;
        }

        return await ResolveSubscriptionCoreAsync(
            externalSubscriptionId,
            ignoreUnmanagedSubscription: true,
            expectedBillingOperationId: null,
            cancellationToken);
    }

    public async Task<AuthoritativeCommercialSubscription> ResolveCheckoutSubscriptionAsync(
        string externalSubscriptionId,
        Guid expectedBillingOperationId,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(externalSubscriptionId))
        {
            throw new ArgumentException(
                "An external subscription identifier is required.",
                nameof(externalSubscriptionId));
        }

        try
        {
            return await ResolveSubscriptionCoreAsync(
                    externalSubscriptionId,
                    ignoreUnmanagedSubscription: false,
                    expectedBillingOperationId,
                    cancellationToken)
                ?? throw new InvalidOperationException(
                    "The completed Checkout subscription is not managed by Styrhous.");
        }
        catch (TaskCanceledException exception) when (!cancellationToken.IsCancellationRequested)
        {
            throw CheckoutProviderUnavailable(exception);
        }
        catch (HttpRequestException exception)
        {
            throw CheckoutProviderUnavailable(exception);
        }
        catch (StripeException exception)
        {
            throw CheckoutProviderUnavailable(exception);
        }
    }

    private async Task<AuthoritativeCommercialSubscription?> ResolveSubscriptionCoreAsync(
        string externalSubscriptionId,
        bool ignoreUnmanagedSubscription,
        Guid? expectedBillingOperationId,
        CancellationToken cancellationToken)
    {
        var subscription = await _subscriptions.GetAsync(
            externalSubscriptionId,
            options: null,
            requestOptions: null,
            cancellationToken);
        var observedAt = RequireProviderObservedAt(subscription);
        var providerReadRevision = await providerReadRevisions.ReserveAsync(
            cancellationToken);
        if (!string.Equals(subscription.Id, externalSubscriptionId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Stripe returned a different subscription than the one requested.");
        }

        return ResolveSubscription(
            subscription,
            ignoreUnmanagedSubscription,
            expectedBillingOperationId,
            observedAt,
            providerReadRevision);
    }

    private AuthoritativeCommercialSubscription? ResolveSubscription(
        Subscription subscription,
        bool ignoreUnmanagedSubscription,
        Guid? expectedBillingOperationId,
        DateTimeOffset observedAt,
        long providerReadRevision,
        CommercialSubscriptionSnapshotKind snapshotKind =
            CommercialSubscriptionSnapshotKind.Observation)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(providerReadRevision);
        if (!subscription.Metadata.TryGetValue(
                StripeBillingMetadata.BillingAccountIdKey,
                out var billingAccountValue))
        {
            return ignoreUnmanagedSubscription
                ? null
                : throw new InvalidOperationException(
                    "The Stripe subscription has no Styrhous billing-account metadata.");
        }

        if (!Guid.TryParse(billingAccountValue, out var billingAccountId))
        {
            throw new InvalidOperationException(
                "Stripe subscription billing-account metadata is invalid.");
        }

        Guid? originatingOperationId = null;
        if (subscription.Metadata.TryGetValue(StripeBillingMetadata.BillingOperationIdKey, out var operationValue)
            && Guid.TryParse(operationValue, out var parsedOperationId))
        {
            originatingOperationId = parsedOperationId;
        }

        if (expectedBillingOperationId is not null)
        {
            if (!subscription.Metadata.TryGetValue(
                    StripeBillingMetadata.BillingOperationIdKey,
                    out var billingOperationValue)
                || !Guid.TryParse(billingOperationValue, out var billingOperationId)
                || billingOperationId != expectedBillingOperationId)
            {
                throw new InvalidOperationException(
                    "Stripe subscription billing-operation metadata does not match "
                        + "the completed Checkout operation.");
            }
        }

        var item = ResolveSingleSubscriptionItem(subscription);
        var priceId = RequiredProviderValue(item.Price?.Id, "price");
        if (!_allowedPriceIds.Contains(priceId))
        {
            throw new InvalidOperationException(
                "Stripe subscription price is not configured for Styrhous licensing.");
        }

        var quantity = item.Quantity;
        if (quantity <= 0 || quantity > int.MaxValue)
        {
            throw new InvalidOperationException(
                "Stripe subscription seat quantity is invalid.");
        }

        var currentPeriodStartedAt = new DateTimeOffset(item.CurrentPeriodStart)
            .ToUniversalTime();
        var currentPeriodEndsAt = new DateTimeOffset(item.CurrentPeriodEnd)
            .ToUniversalTime();
        if (currentPeriodEndsAt <= currentPeriodStartedAt)
        {
            throw new InvalidOperationException(
                "Stripe subscription current period is invalid.");
        }

        return new AuthoritativeCommercialSubscription(
            billingAccountId,
            new CommercialSubscriptionProjection(
                RequiredProviderValue(subscription.CustomerId, "customer"),
                RequiredProviderValue(subscription.Id, "subscription"),
                priceId,
                MapStatus(subscription.Status),
                checked((int)quantity),
                subscription.CancelAtPeriodEnd,
                currentPeriodStartedAt,
                currentPeriodEndsAt,
                observedAt),
            providerReadRevision,
            snapshotKind,
            originatingOperationId);
    }

    private static void ValidateSeatQuantityRequest(
        BillingSeatQuantityProviderRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        StripeBillingOptions.RequireCanonicalExternalIdentifier(
            request.ExternalSubscriptionId,
            nameof(request.ExternalSubscriptionId));
        if (request.PreviousSeatQuantity <= 0
            || request.SeatQuantity <= 0
            || request.PreviousSeatQuantity == request.SeatQuantity)
        {
            throw new ArgumentException(
                "Seat-quantity changes require distinct positive quantities.",
                nameof(request));
        }
    }

    private static void EnsureSeatQuantitySubscription(
        BillingSeatQuantityProviderRequest request,
        Subscription providerSubscription,
        AuthoritativeCommercialSubscription authoritative)
    {
        if (!string.Equals(
                providerSubscription.Id,
                request.ExternalSubscriptionId,
                StringComparison.Ordinal)
            || authoritative.BillingAccountId != request.BillingAccountId)
        {
            throw new InvalidOperationException(
                "Stripe returned a different subscription than the requested managed account.");
        }
    }

    private static BillingCheckoutProviderUnavailableException CheckoutProviderUnavailable(
        Exception exception)
    {
        return new("Stripe subscription reconciliation is temporarily unavailable.", exception);
    }

    private static string? ResolveSubscriptionId(
        Event providerEvent,
        BillingWebhookEventKind kind)
    {
        return kind switch
        {
            BillingWebhookEventKind.CheckoutCompleted
                when providerEvent.Data.Object is Stripe.Checkout.Session session =>
                    session.SubscriptionId,
            BillingWebhookEventKind.SubscriptionChanged
                when providerEvent.Data.Object is Subscription subscription =>
                    subscription.Id,
            BillingWebhookEventKind.InvoicePaid or BillingWebhookEventKind.PaymentFailed
                when providerEvent.Data.Object is Invoice invoice =>
                    invoice.Parent?.SubscriptionDetails?.SubscriptionId,
            _ => throw new InvalidOperationException(
                "Stripe event data does not match the persisted billing event kind."),
        };
    }

    private static SubscriptionItem ResolveSingleSubscriptionItem(
        Subscription subscription)
    {
        var items = subscription.Items?.Data;
        if (items is null || items.Count != 1)
        {
            throw new InvalidOperationException(
                "A managed Stripe subscription must have exactly one item.");
        }

        return items[0];
    }

    private static CommercialSubscriptionStatus MapStatus(string status)
    {
        return status switch
        {
            "active" => CommercialSubscriptionStatus.Active,
            "past_due" => CommercialSubscriptionStatus.PastDue,
            "unpaid" => CommercialSubscriptionStatus.Unpaid,
            "paused" => CommercialSubscriptionStatus.Paused,
            "incomplete" => CommercialSubscriptionStatus.Incomplete,
            "incomplete_expired" => CommercialSubscriptionStatus.IncompleteExpired,
            "trialing" => CommercialSubscriptionStatus.Trialing,
            "canceled" => CommercialSubscriptionStatus.Canceled,
            _ => throw new InvalidOperationException(
                "Stripe returned an unsupported subscription status."),
        };
    }

    private static HashSet<string> CreateAllowedPriceIds(StripeBillingOptions options)
    {
        var (monthlyPriceId, annualPriceId) = options.GetValidatedPriceIds();

        return new HashSet<string>([monthlyPriceId, annualPriceId], StringComparer.Ordinal);
    }

    private static string RequiredProviderValue(string? value, string fieldName)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new InvalidOperationException(
                $"Stripe subscription {fieldName} identifier is missing.");
        }

        return value;
    }

    private static DateTimeOffset RequireProviderObservedAt(Subscription subscription)
    {
        return subscription.StripeResponse?.Date?.ToUniversalTime()
        ?? throw new InvalidOperationException(
            "Stripe did not return its authoritative response time.");
    }

    private static BillingSeatQuantityProviderUnavailableException
        SeatQuantityProviderUnavailable(
        Exception exception)
    {
        return new("Stripe seat-quantity management is temporarily unavailable.", exception);
    }

    [LoggerMessage(
        EventId = 2411,
        Level = LogLevel.Error,
        Message = "Stripe permanently rejected a seat-quantity change with status "
            + "{StripeStatusCode}, error type {StripeErrorType}, error code {StripeErrorCode}, "
            + "and request {StripeRequestId}.")]
    private static partial void LogPermanentStripeRejection(
        ILogger logger,
        HttpStatusCode stripeStatusCode,
        string? stripeErrorType,
        string? stripeErrorCode,
        string? stripeRequestId);

    [LoggerMessage(
        EventId = 2412,
        Level = LogLevel.Warning,
        Message = "Stripe seat-quantity management had a transient {FailureKind} failure.")]
    private static partial void LogTransientProviderFailure(
        ILogger logger,
        string failureKind);

    [LoggerMessage(
        EventId = 2413,
        Level = LogLevel.Warning,
        Message = "Stripe transiently rejected a seat-quantity change with status "
            + "{StripeStatusCode}, error type {StripeErrorType}, error code {StripeErrorCode}, "
            + "and request {StripeRequestId}.")]
    private static partial void LogTransientStripeFailure(
        ILogger logger,
        HttpStatusCode stripeStatusCode,
        string? stripeErrorType,
        string? stripeErrorCode,
        string? stripeRequestId);
}
