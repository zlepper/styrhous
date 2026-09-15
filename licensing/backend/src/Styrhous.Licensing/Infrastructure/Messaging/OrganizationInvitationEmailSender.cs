using System.Globalization;
using System.Net;
using Microsoft.AspNetCore.WebUtilities;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed class OrganizationInvitationEmailSender(
    IEmailSubmissionClient client,
    InvitationEmailSettings settings)
    : IOrganizationInvitationEmailSender
{
    private readonly IEmailSubmissionClient _client = client;
    private readonly InvitationEmailSettings _settings = settings;

    public Task SendAsync(
        Guid outboxMessageId,
        OrganizationInvitationDelivery delivery,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(delivery);
        var acceptanceUrl = QueryHelpers.AddQueryString(
            _settings.AcceptanceUrl.AbsoluteUri,
            "secret",
            delivery.Secret.Reveal());
        var role = delivery.Role == OrganizationRole.Admin
            ? "an administrator"
            : "a member";
        var expiresAt = delivery.ExpiresAt.ToUniversalTime().ToString(
            "yyyy-MM-dd HH:mm 'UTC'",
            CultureInfo.InvariantCulture);
        var subject = delivery.Kind == OrganizationInvitationDeliveryKind.Resent
            ? "Your updated invitation to Styrhous"
            : "You have been invited to Styrhous";
        var textBody = $"""
            You have been invited to join a Styrhous organization as {role}.

            Accept the invitation: {acceptanceUrl}

            This invitation expires at {expiresAt}.
            """;
        var htmlBody = $"""
            <p>You have been invited to join a Styrhous organization as {role}.</p>
            <p><a href="{WebUtility.HtmlEncode(acceptanceUrl)}">Accept the invitation</a></p>
            <p>This invitation expires at {expiresAt}.</p>
            """;
        return _client.SendAsync(
            new InvitationEmailSubmission(
                outboxMessageId,
                _settings.FromAddress,
                delivery.Email,
                subject,
                textBody,
                htmlBody),
            cancellationToken);
    }
}
