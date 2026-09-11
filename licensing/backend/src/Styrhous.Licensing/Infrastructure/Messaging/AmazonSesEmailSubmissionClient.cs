using Amazon.SimpleEmailV2;
using Amazon.SimpleEmailV2.Model;

namespace Styrhous.Licensing.Infrastructure.Messaging;

internal sealed class AmazonSesEmailSubmissionClient(
    IAmazonSimpleEmailServiceV2 client)
    : IEmailSubmissionClient
{

    public async Task SendAsync(
        InvitationEmailSubmission submission,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(submission);
        await client.SendEmailAsync(CreateRequest(submission), cancellationToken);
    }

    internal static SendEmailRequest CreateRequest(InvitationEmailSubmission submission)
    {
        ArgumentNullException.ThrowIfNull(submission);
        return new SendEmailRequest
        {
            FromEmailAddress = submission.FromAddress,
            Destination = new Destination
            {
                ToAddresses = [submission.ToAddress],
            },
            Content = new EmailContent
            {
                Simple = new Message
                {
                    Subject = Utf8(submission.Subject),
                    Body = new Body
                    {
                        Text = Utf8(submission.TextBody),
                        Html = Utf8(submission.HtmlBody),
                    },
                },
            },
            EmailTags =
            [
                new MessageTag
                {
                    Name = "styrhous-delivery-id",
                    Value = submission.DeliveryId.ToString(),
                },
            ],
        };
    }

    private static Content Utf8(string data)
    {
        return new()
        {
            Charset = "UTF-8",
            Data = data,
        };
    }
}
