using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Billing;

public sealed class BillingCheckoutService(
    PostgresBillingCheckoutStore store,
    IBillingCheckoutProvider provider,
    BillingCheckoutCompletionReconciler completionReconciler,
    TimeProvider timeProvider)
{
    private const int MaximumPreparationAttempts = 4;

    public async Task<BillingCheckoutResult> CreateSessionAsync(
        Guid actorUserId,
        Guid billingAccountId,
        BillingCadence cadence,
        int seatQuantity,
        Guid? retryOperationId,
        CancellationToken cancellationToken = default)
    {
        Validate(cadence, seatQuantity);
        var effectiveRetryOperationId = retryOperationId;
        for (var attempt = 0; attempt < MaximumPreparationAttempts; attempt++)
        {
            var preparation = await store.PrepareAsync(
                actorUserId,
                billingAccountId,
                cadence,
                seatQuantity,
                effectiveRetryOperationId,
                timeProvider.GetUtcNow(),
                cancellationToken);
            if (preparation.Status
                == BillingCheckoutPreparationStatus.ProviderSessionReconciliationRequired)
            {
                var reconciliationStep = await ReconcileAsync(
                    preparation,
                    cancellationToken);
                if (reconciliationStep.Result is not null)
                {
                    return reconciliationStep.Result;
                }

                if (reconciliationStep.RetryWithoutOperation)
                {
                    effectiveRetryOperationId = null;
                }

                continue;
            }

            if (preparation.Status != BillingCheckoutPreparationStatus.Prepared)
            {
                return new BillingCheckoutResult(
                    Map(preparation.Status),
                    preparation.RequiredSeatQuantity,
                    ToAttempt(preparation.Operation),
                    RedirectUri: null);
            }

            var creationStep = await CreateProviderSessionAsync(
                preparation,
                cancellationToken);
            if (creationStep.Result is not null)
            {
                return creationStep.Result;
            }

            if (creationStep.RetryWithoutOperation)
            {
                effectiveRetryOperationId = null;
            }
        }

        throw new InvalidOperationException(
            "Checkout preparation did not converge after provider reconciliation.");
    }

    private async Task<BillingCheckoutStep> CreateProviderSessionAsync(
        BillingCheckoutPreparationResult preparation,
        CancellationToken cancellationToken)
    {
        var operation = preparation.Operation
            ?? throw new InvalidOperationException(
                "A prepared Checkout operation must contain its durable operation.");
        BillingCheckoutProviderSession providerSession;
        try
        {
            providerSession = await provider.CreateSessionAsync(
                new BillingCheckoutProviderRequest(
                    operation.Id,
                    operation.BillingAccountId,
                    operation.RequireCheckoutCadence(),
                    operation.SeatQuantity,
                    operation.RequireCheckoutExpiry(),
                    operation.PreviousSubscription?.ExternalCustomerId),
                cancellationToken);
        }
        catch (BillingCheckoutProviderSessionCreationRejectedException)
        {
            if (!await store.FailProviderSessionCreationAsync(
                    operation.Id,
                    timeProvider.GetUtcNow(),
                    cancellationToken))
            {
                throw new InvalidOperationException(
                    "The rejected Checkout operation could not be closed.");
            }

            return BillingCheckoutStep.RetryFresh();
        }
        catch (BillingCheckoutProviderUnavailableException)
        {
            return BillingCheckoutStep.Return(
                ProviderUnavailable(preparation, operation));
        }

        if (!providerSession.RedirectUri.IsAbsoluteUri
            || !string.Equals(
                providerSession.RedirectUri.Scheme,
                Uri.UriSchemeHttps,
                StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "The billing provider returned an invalid Checkout redirect URI.");
        }

        if (!await store.RecordProviderSessionAsync(
                operation.Id,
                providerSession.ExternalSessionId,
                timeProvider.GetUtcNow(),
                cancellationToken))
        {
            throw new InvalidOperationException(
                "The Checkout provider session could not be recorded.");
        }

        if (timeProvider.GetUtcNow() >= operation.RequireCheckoutExpiry())
        {
            return BillingCheckoutStep.RetryCurrent();
        }

        return BillingCheckoutStep.Return(
            new BillingCheckoutResult(
                BillingCheckoutStatus.Created,
                preparation.RequiredSeatQuantity,
                ToAttempt(operation),
                providerSession.RedirectUri));
    }

    private async Task<BillingCheckoutStep> ReconcileAsync(
        BillingCheckoutPreparationResult preparation,
        CancellationToken cancellationToken)
    {
        var operation = preparation.Operation
            ?? throw new InvalidOperationException(
                "Checkout reconciliation requires its durable operation.");
        if (operation.Status == BillingOperationStatus.Pending)
        {
            return await CreateProviderSessionAsync(preparation, cancellationToken);
        }

        if (operation.Status != BillingOperationStatus.ProviderSessionCreated
            || operation.ExternalSessionId is null)
        {
            throw new InvalidOperationException(
                "Only a pending or provider-backed Checkout can be reconciled.");
        }

        BillingCheckoutProviderSessionState providerState;
        try
        {
            providerState = await provider.GetSessionStateAsync(
                operation.ExternalSessionId,
                cancellationToken);
        }
        catch (BillingCheckoutProviderUnavailableException)
        {
            return BillingCheckoutStep.Return(
                ProviderUnavailable(preparation, operation));
        }

        if (providerState.Status == BillingCheckoutProviderSessionStatus.Complete)
        {
            try
            {
                await completionReconciler.ReconcileAsync(
                    operation.BillingAccountId,
                    operation.Id,
                    providerState.ExternalSubscriptionId!,
                    cancellationToken);
            }
            catch (BillingCheckoutProviderUnavailableException)
            {
                return BillingCheckoutStep.Return(
                    ProviderUnavailable(preparation, operation));
            }

            return BillingCheckoutStep.RetryCurrent();
        }

        if (providerState.Status != BillingCheckoutProviderSessionStatus.Expired)
        {
            return BillingCheckoutStep.Return(
                new BillingCheckoutResult(
                    BillingCheckoutStatus.CheckoutOperationInProgress,
                    preparation.RequiredSeatQuantity,
                    ToAttempt(operation),
                    RedirectUri: null));
        }

        if (!await store.ExpireProviderSessionAsync(
                operation.Id,
                operation.ExternalSessionId,
                timeProvider.GetUtcNow(),
                cancellationToken))
        {
            throw new InvalidOperationException(
                "The expired Checkout operation could not be closed.");
        }

        return BillingCheckoutStep.RetryFresh();
    }

    private static BillingCheckoutResult ProviderUnavailable(
        BillingCheckoutPreparationResult preparation,
        BillingOperation operation)
    {
        return new(
            BillingCheckoutStatus.ProviderUnavailable,
            preparation.RequiredSeatQuantity,
            ToAttempt(operation),
            RedirectUri: null);
    }

    private static void Validate(BillingCadence cadence, int seatQuantity)
    {

        if (!Enum.IsDefined(cadence))
        {
            throw new ArgumentOutOfRangeException(nameof(cadence));
        }

        if (seatQuantity <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(seatQuantity),
                "Checkout requires at least one seat.");
        }
    }

    private static BillingCheckoutStatus Map(BillingCheckoutPreparationStatus status)
    {
        return status switch
        {
            BillingCheckoutPreparationStatus.BillingAccountNotFound =>
                BillingCheckoutStatus.BillingAccountNotFound,
            BillingCheckoutPreparationStatus.BillingOperationNotFound =>
                BillingCheckoutStatus.BillingOperationNotFound,
            BillingCheckoutPreparationStatus.InsufficientPermission =>
                BillingCheckoutStatus.InsufficientPermission,
            BillingCheckoutPreparationStatus.PersonalSeatQuantityInvalid =>
                BillingCheckoutStatus.PersonalSeatQuantityInvalid,
            BillingCheckoutPreparationStatus.SeatQuantityTooSmall =>
                BillingCheckoutStatus.SeatQuantityTooSmall,
            BillingCheckoutPreparationStatus.SubscriptionAlreadyExists =>
                BillingCheckoutStatus.SubscriptionAlreadyExists,
            BillingCheckoutPreparationStatus.CheckoutOperationInProgress =>
                BillingCheckoutStatus.CheckoutOperationInProgress,
            BillingCheckoutPreparationStatus.CheckoutOperationCapacityChanged =>
                BillingCheckoutStatus.CheckoutOperationCapacityChanged,
            BillingCheckoutPreparationStatus.ProviderSessionReconciliationRequired =>
                throw new InvalidOperationException(
                    "Provider reconciliation must be handled before result mapping."),
            BillingCheckoutPreparationStatus.Prepared =>
                throw new InvalidOperationException(
                    "A prepared Checkout operation cannot be mapped as a rejection."),
            _ => throw new ArgumentOutOfRangeException(nameof(status)),
        };
    }

    private static BillingCheckoutAttempt? ToAttempt(BillingOperation? operation)
    {
        return operation is null
            ? null
            : new BillingCheckoutAttempt(
                operation.Id,
                operation.RequireCheckoutCadence(),
                operation.SeatQuantity,
                operation.RequireCheckoutExpiry());
    }

    private sealed record BillingCheckoutStep(
        BillingCheckoutResult? Result,
        bool RetryWithoutOperation)
    {
        public static BillingCheckoutStep Return(BillingCheckoutResult result)
        {
            return new(result, RetryWithoutOperation: false);
        }

        public static BillingCheckoutStep RetryCurrent()
        {
            return new(Result: null, RetryWithoutOperation: false);
        }

        public static BillingCheckoutStep RetryFresh()
        {
            return new(Result: null, RetryWithoutOperation: true);
        }
    }
}
