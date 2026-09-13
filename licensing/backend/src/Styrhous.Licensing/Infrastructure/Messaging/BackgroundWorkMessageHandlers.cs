using Rebus.Extensions;
using Rebus.Handlers;
using Rebus.Pipeline;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Messaging;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed class OrganizationInvitationDeliveryMessageHandler(
    OrganizationInvitationDeliveryService deliveryService)
    : IHandleMessages<OrganizationInvitationDeliveryMessage>
{

    public async Task Handle(OrganizationInvitationDeliveryMessage message)
    {
        ArgumentNullException.ThrowIfNull(message);
        await BusyBackgroundWorkRetry.RunAsync(async cancellationToken =>
        {
            var result = await deliveryService.DeliverAsync(message.WorkId, cancellationToken);
            return result.Status != OrganizationInvitationDeliveryProcessingStatus.Busy;
        }, RebusMessageContext.ShutdownToken);
    }
}

internal sealed class BillingWebhookProcessingMessageHandler(
    BillingWebhookProcessingService processingService)
    : IHandleMessages<BillingWebhookProcessingMessage>
{

    public async Task Handle(BillingWebhookProcessingMessage message)
    {
        ArgumentNullException.ThrowIfNull(message);
        await BusyBackgroundWorkRetry.RunAsync(async cancellationToken =>
        {
            var result = await processingService.ProcessAsync(message.WorkId, cancellationToken);
            return result.Status != BillingWebhookProcessingStatus.Busy;
        }, RebusMessageContext.ShutdownToken);
    }
}

internal sealed class InfrastructureSmokeProbeMessageHandler(
    InfrastructureSmokeProbeProcessor processor)
    : IHandleMessages<InfrastructureSmokeProbeMessage>
{

    public async Task Handle(InfrastructureSmokeProbeMessage message)
    {
        ArgumentNullException.ThrowIfNull(message);
        await BusyBackgroundWorkRetry.RunAsync(async cancellationToken =>
        {
            return await processor.ProcessAsync(message.WorkId, cancellationToken)
                != InfrastructureSmokeProbeProcessingStatus.Busy;
        }, RebusMessageContext.ShutdownToken);
    }
}

file static class RebusMessageContext
{
    public static CancellationToken ShutdownToken =>
        MessageContext.Current?.GetCancellationToken() ?? CancellationToken.None;
}
