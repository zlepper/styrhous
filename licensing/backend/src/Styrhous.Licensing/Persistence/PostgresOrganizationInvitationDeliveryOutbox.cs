using Styrhous.Licensing.Infrastructure.Messaging;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresOrganizationInvitationDeliveryOutbox
{
    public static OutboxMessage Enqueue(
        LicensingDbContext dbContext,
        DataProtectionOrganizationInvitationDeliveryProtector deliveryProtector,
        OrganizationInvitationDeliveryKind kind,
        OrganizationInvitation invitation,
        OrganizationInvitationSecret secret,
        Guid correlationId,
        DateTimeOffset occurredAt)
    {
        var message = OutboxMessage.Enqueue(
            correlationId,
            invitation.Id,
            OutboxMessageTypes.OrganizationInvitationDelivery,
            deliveryProtector.Protect(
                OrganizationInvitationDelivery.From(kind, invitation, secret)),
            occurredAt,
            invitation.ExpiresAt);
        dbContext.OutboxMessages.Add(message);
        return message;
    }

    public static Task<int> DiscardPendingAsync(
        LicensingDbContext dbContext,
        Guid invitationId,
        OutboxDiscardReason reason,
        DateTimeOffset discardedAt,
        CancellationToken cancellationToken)
    {
        return dbContext.OutboxMessages
            .Where(message => message.SubjectId == invitationId
                && message.MessageType
                    == OutboxMessageTypes.OrganizationInvitationDelivery
                && message.DeliveredAt == null
                && message.DiscardedAt == null)
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(
                        message => message.DiscardedAt,
                        discardedAt.ToUniversalTime())
                    .SetProperty(message => message.DiscardReason, reason)
                    .SetProperty(message => message.ProcessingLeaseId, (Guid?)null)
                    .SetProperty(
                        message => message.ProcessingLeaseExpiresAt,
                        (DateTimeOffset?)null),
                cancellationToken);
    }
}
