namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed class InvitationEmailSubmission(
    Guid deliveryId,
    string fromAddress,
    string toAddress,
    string subject,
    string textBody,
    string htmlBody)
{
    private const string RedactedValue = "[REDACTED INVITATION EMAIL]";

    public Guid DeliveryId { get; } = deliveryId;

    public string FromAddress { get; } = fromAddress;

    public string ToAddress { get; } = toAddress;

    public string Subject { get; } = subject;

    public string TextBody { get; } = textBody;

    public string HtmlBody { get; } = htmlBody;

    public override string ToString()
    {
        return RedactedValue;
    }
}

internal interface IEmailSubmissionClient
{
    Task SendAsync(
        InvitationEmailSubmission submission,
        CancellationToken cancellationToken);
}
