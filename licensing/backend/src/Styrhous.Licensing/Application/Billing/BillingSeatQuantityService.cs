using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public sealed class BillingSeatQuantityService(
    PostgresBillingSeatQuantityStore store,
    IBillingSeatQuantityProvider provider,
    TimeProvider timeProvider)
{
    private static readonly TimeSpan ProviderMutationAutomaticReplayLifetime =
        TimeSpan.FromHours(23);

    public async Task<BillingSeatQuantityResult> ChangeAsync(
        Guid actorUserId,
        Guid billingAccountId,
        int seatQuantity,
        Guid? retryOperationId,
        CancellationToken cancellationToken = default)
    {

        if (seatQuantity <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(seatQuantity),
                "At least one seat is required.");
        }

        var preparation = await store.PrepareAsync(
            actorUserId,
            billingAccountId,
            seatQuantity,
            retryOperationId,
            timeProvider.GetUtcNow(),
            cancellationToken);
        if (preparation.Status is BillingSeatQuantityPreparationStatus.Completed
            or BillingSeatQuantityPreparationStatus.Failed)
        {
            return Terminal(preparation);
        }

        if (preparation.Status != BillingSeatQuantityPreparationStatus.Prepared)
        {
            return Rejected(preparation);
        }

        var operation = preparation.Operation
            ?? throw new InvalidOperationException(
                "A prepared seat-quantity change requires its durable operation.");
        var previousSeatQuantity = operation.RequirePreviousSeatQuantity();
        var providerRequest = new BillingSeatQuantityProviderRequest(
            operation.Id,
            operation.BillingAccountId,
            preparation.ExternalSubscriptionId
                ?? throw new InvalidOperationException(
                    "A prepared seat-quantity change requires its subscription."),
            previousSeatQuantity,
            operation.SeatQuantity);
        BillingSeatQuantityResolutionResult? resolution;
        var providerReplayWindowElapsed = false;
        try
        {
            var observation = await provider.ObserveAsync(
                providerRequest,
                cancellationToken);
            ValidateProviderResult(operation, observation);
            if (observation.Status == BillingSeatQuantityProviderStatus.MutationRequired)
            {
                var mutation = await store.ApplyObservationAndPrepareProviderMutationAsync(
                        operation.Id,
                        observation.Subscription,
                        cancellationToken);
                if (mutation is null)
                {
                    throw new InvalidOperationException(
                        "The observed seat-quantity operation could not be persisted.");
                }

                if (mutation.OperationStatus is BillingOperationStatus.Completed
                    or BillingOperationStatus.Failed)
                {
                    return Terminal(
                        operation,
                        mutation.OperationStatus,
                        mutation.Outcome,
                        preparation.RequiredSeatQuantity,
                        mutation.AuthoritativeSeatQuantity);
                }

                var replayStartedAt = mutation.ProviderMutationReplayStartedAt
                    ?? throw new InvalidOperationException(
                        "A prepared provider mutation requires its replay start time.");
                var automaticReplayEndsAt = replayStartedAt.Add(
                    ProviderMutationAutomaticReplayLifetime);
                if (mutation.AuthoritativeProviderObservedAt >= automaticReplayEndsAt)
                {
                    return new BillingSeatQuantityResult(
                        BillingSeatQuantityStatus.ProviderReconciliationRequired,
                        ToOperation(operation),
                        preparation.RequiredSeatQuantity,
                        mutation.AuthoritativeSeatQuantity);
                }

                resolution = await store.ApplyProviderMutationIfObservationCurrentAsync(
                    operation.Id,
                    observation.Subscription,
                    async token =>
                    {
                        var providerResult = await provider.ApplyAsync(
                            providerRequest,
                            automaticReplayEndsAt,
                            token);
                        ValidateProviderResult(operation, providerResult);
                        if (providerResult.Status
                            == BillingSeatQuantityProviderStatus.MutationRequired)
                        {
                            providerReplayWindowElapsed = true;
                        }

                        return providerResult.Subscription;
                    },
                    LaterOf(operation.CreatedAt, timeProvider.GetUtcNow()),
                    cancellationToken);
                if (resolution is null)
                {
                    throw new InvalidOperationException(
                        "The seat-quantity operation could not be resolved.");
                }
            }
            else
            {
                resolution = await store.ApplySubscriptionAndResolveAsync(
                    operation.Id,
                    observation.Subscription,
                    LaterOf(operation.CreatedAt, timeProvider.GetUtcNow()),
                    cancellationToken);
                if (resolution is null)
                {
                    throw new InvalidOperationException(
                        "The seat-quantity operation could not be resolved.");
                }
            }
        }
        catch (BillingSeatQuantityProviderUnavailableException)
        {
            return ProviderUnavailable(operation, preparation.RequiredSeatQuantity);
        }
        catch (BillingSeatQuantityProviderIndeterminateException)
        {
            return ProviderUnavailable(operation, preparation.RequiredSeatQuantity);
        }
        catch (BillingSeatQuantityProviderRejectedException)
        {
            var rejectedResolution = await store.RejectProviderMutationAsync(
                    operation.Id,
                    LaterOf(operation.CreatedAt, timeProvider.GetUtcNow()),
                    cancellationToken);
            if (rejectedResolution is null)
            {
                throw new InvalidOperationException(
                    "The rejected seat-quantity operation could not be closed.");
            }

            return Terminal(
                operation,
                rejectedResolution.OperationStatus,
                rejectedResolution.Outcome,
                preparation.RequiredSeatQuantity,
                rejectedResolution.AuthoritativeSeatQuantity);
        }

        if (resolution.OperationStatus == BillingOperationStatus.Pending)
        {
            if (providerReplayWindowElapsed)
            {
                return new BillingSeatQuantityResult(
                    BillingSeatQuantityStatus.ProviderReconciliationRequired,
                    ToOperation(operation),
                    preparation.RequiredSeatQuantity,
                    resolution.AuthoritativeSeatQuantity);
            }

            return ProviderUnavailable(operation, preparation.RequiredSeatQuantity);
        }

        return new BillingSeatQuantityResult(
            ToStatus(
                resolution.OperationStatus,
                resolution.Outcome ?? throw new InvalidOperationException(
                    "A terminal seat-quantity operation requires its outcome.")),
            ToOperation(operation),
            preparation.RequiredSeatQuantity,
            resolution.AuthoritativeSeatQuantity);
    }

    private static void ValidateProviderResult(
        BillingOperation operation,
        BillingSeatQuantityProviderResult result)
    {
        ArgumentNullException.ThrowIfNull(result);
        ArgumentNullException.ThrowIfNull(result.Subscription);
        if (!Enum.IsDefined(result.Status))
        {
            throw new InvalidOperationException(
                "The seat-quantity provider returned an unknown result status.");
        }

        if (result.Subscription.BillingAccountId != operation.BillingAccountId)
        {
            throw new InvalidOperationException(
                "The seat-quantity provider returned a different billing account.");
        }

        var returnedQuantity = result.Subscription.Projection.SeatQuantity;
        if (result.Status == BillingSeatQuantityProviderStatus.MutationRequired
            && returnedQuantity != operation.PreviousSeatQuantity)
        {
            throw new InvalidOperationException(
                "A required seat-quantity mutation must contain the previous quantity.");
        }

        if (result.Status == BillingSeatQuantityProviderStatus.Applied
            && returnedQuantity != operation.SeatQuantity)
        {
            throw new InvalidOperationException(
                "The seat-quantity provider did not return the requested quantity.");
        }

    }

    private static BillingSeatQuantityResult Rejected(
        BillingSeatQuantityPreparationResult preparation)
    {
        return new(
            Map(preparation.Status),
            ToOperation(preparation.Operation),
            preparation.RequiredSeatQuantity,
            AuthoritativeSeatQuantity: null);
    }

    private static BillingSeatQuantityResult ProviderUnavailable(
        BillingOperation operation,
        int? requiredSeatQuantity)
    {
        return new(
            BillingSeatQuantityStatus.ProviderUnavailable,
            ToOperation(operation),
            requiredSeatQuantity,
            AuthoritativeSeatQuantity: null);
    }

    private static BillingSeatQuantityResult Terminal(
        BillingSeatQuantityPreparationResult preparation)
    {
        var operation = preparation.Operation
            ?? throw new InvalidOperationException(
                "A terminal seat-quantity preparation requires its durable operation.");
        var authoritativeSeatQuantity = preparation.AuthoritativeSeatQuantity
            ?? throw new InvalidOperationException(
                "A terminal seat-quantity preparation requires its current quantity.");
        return new BillingSeatQuantityResult(
            ToStatus(operation.Status, operation.RequireSeatQuantityOutcome()),
            ToOperation(operation),
            preparation.RequiredSeatQuantity,
            authoritativeSeatQuantity);
    }

    private static BillingSeatQuantityResult Terminal(
        BillingOperation operation,
        BillingOperationStatus operationStatus,
        SeatQuantityChangeOutcome? outcome,
        int? requiredSeatQuantity,
        int authoritativeSeatQuantity)
    {
        return new(
            ToStatus(
                operationStatus,
                outcome ?? throw new InvalidOperationException(
                    "A terminal seat-quantity operation requires its outcome.")),
            ToOperation(operation),
            requiredSeatQuantity,
            authoritativeSeatQuantity);
    }

    private static BillingSeatQuantityStatus ToStatus(
        BillingOperationStatus operationStatus,
        SeatQuantityChangeOutcome outcome)
    {
        if (operationStatus == BillingOperationStatus.Completed
            && outcome == SeatQuantityChangeOutcome.Applied)
        {
            return BillingSeatQuantityStatus.Changed;
        }

        if (operationStatus == BillingOperationStatus.Failed
            && outcome == SeatQuantityChangeOutcome.Superseded)
        {
            return BillingSeatQuantityStatus.SubscriptionQuantityChanged;
        }

        if (operationStatus == BillingOperationStatus.Failed
            && outcome == SeatQuantityChangeOutcome.ProviderRejected)
        {
            throw new InvalidOperationException(
                "The configured billing provider rejected the seat-quantity change.");
        }

        throw new InvalidOperationException(
            "The seat-quantity operation has an inconsistent terminal outcome.");
    }

    private static DateTimeOffset LaterOf(
        DateTimeOffset first,
        DateTimeOffset second)
    {
        return first >= second ? first : second;
    }

    private static BillingSeatQuantityOperation? ToOperation(
        BillingOperation? operation)
    {
        return operation is null
            ? null
            : new BillingSeatQuantityOperation(
                operation.Id,
                operation.RequirePreviousSeatQuantity(),
                operation.SeatQuantity);
    }

    private static BillingSeatQuantityStatus Map(
        BillingSeatQuantityPreparationStatus status)
    {
        return status switch
        {
            BillingSeatQuantityPreparationStatus.BillingAccountNotFound =>
                BillingSeatQuantityStatus.BillingAccountNotFound,
            BillingSeatQuantityPreparationStatus.BillingOperationNotFound =>
                BillingSeatQuantityStatus.BillingOperationNotFound,
            BillingSeatQuantityPreparationStatus.InsufficientPermission =>
                BillingSeatQuantityStatus.InsufficientPermission,
            BillingSeatQuantityPreparationStatus.PersonalSeatQuantityInvalid =>
                BillingSeatQuantityStatus.PersonalSeatQuantityInvalid,
            BillingSeatQuantityPreparationStatus.SeatQuantityTooSmall =>
                BillingSeatQuantityStatus.SeatQuantityTooSmall,
            BillingSeatQuantityPreparationStatus.SeatQuantityUnchanged =>
                BillingSeatQuantityStatus.SeatQuantityUnchanged,
            BillingSeatQuantityPreparationStatus.SubscriptionNotFound =>
                BillingSeatQuantityStatus.SubscriptionNotFound,
            BillingSeatQuantityPreparationStatus.SubscriptionInactive =>
                BillingSeatQuantityStatus.SubscriptionInactive,
            BillingSeatQuantityPreparationStatus.OperationInProgress =>
                BillingSeatQuantityStatus.OperationInProgress,
            BillingSeatQuantityPreparationStatus.Prepared =>
                throw new InvalidOperationException(
                    "A prepared seat-quantity change cannot be mapped as a rejection."),
            _ => throw new ArgumentOutOfRangeException(nameof(status)),
        };
    }
}
