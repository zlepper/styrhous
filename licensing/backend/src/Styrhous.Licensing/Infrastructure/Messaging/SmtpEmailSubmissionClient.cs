using MailKit.Net.Smtp;
using MailKit.Security;
using MimeKit;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed class SmtpEmailSubmissionClient(SmtpEmailSettings settings)
    : IEmailSubmissionClient
{
    public async Task SendAsync(
        InvitationEmailSubmission submission,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(submission);
        using var client = new SmtpClient();
        await client.ConnectAsync(
            settings.Host,
            settings.Port,
            SecureSocketOptions.StartTls,
            cancellationToken);
        try
        {
            await client.AuthenticateAsync(
                settings.Username,
                settings.Password,
                cancellationToken);
            await client.SendAsync(CreateMessage(submission), cancellationToken);
        }
        finally
        {
            if (client.IsConnected)
            {
                await client.DisconnectAsync(true, cancellationToken);
            }
        }
    }

    internal static MimeMessage CreateMessage(InvitationEmailSubmission submission)
    {
        ArgumentNullException.ThrowIfNull(submission);
        var message = new MimeMessage();
        message.From.Add(MailboxAddress.Parse(submission.FromAddress));
        message.To.Add(MailboxAddress.Parse(submission.ToAddress));
        message.Subject = submission.Subject;
        message.Headers.Add("X-Styrhous-Delivery-Id", submission.DeliveryId.ToString());
        message.Body = new Multipart("alternative")
        {
            new TextPart("plain") { Text = submission.TextBody },
            new TextPart("html") { Text = submission.HtmlBody },
        };
        return message;
    }
}
