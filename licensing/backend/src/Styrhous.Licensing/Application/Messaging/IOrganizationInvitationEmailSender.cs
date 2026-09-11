namespace Styrhous.Licensing.Application.Messaging;

public interface IOrganizationInvitationEmailSender
{
    Task SendAsync(
        Guid outboxMessageId,
        OrganizationInvitationDelivery delivery,
        CancellationToken cancellationToken);
}
