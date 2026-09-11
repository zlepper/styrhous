using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Infrastructure.Messaging;

public sealed record OrganizationInvitationDeliveryMessage
{
    public OrganizationInvitationDeliveryMessage(Guid workId)
    {

        WorkId = workId;
    }

    public Guid WorkId { get; }

}

public sealed record BillingWebhookProcessingMessage
{
    public BillingWebhookProcessingMessage(Guid workId)
    {

        WorkId = workId;
    }

    public Guid WorkId { get; }
}

public sealed record InfrastructureSmokeProbeMessage
{
    public InfrastructureSmokeProbeMessage(Guid workId)
    {

        WorkId = workId;
    }

    public Guid WorkId { get; }
}
