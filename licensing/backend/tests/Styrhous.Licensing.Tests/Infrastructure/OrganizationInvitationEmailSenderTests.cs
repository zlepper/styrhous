using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Messaging;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationEmailSenderTests
{
    private static readonly DateTimeOffset ExpiresAt =
        new(2026, 9, 8, 12, 30, 0, TimeSpan.Zero);

    [Test]
    public async Task SendsEncodedAcceptanceLinkAndDurableDeliveryIdentifier()
    {
        var deliveryId = Guid.CreateVersion7();
        var client = new RecordingEmailSubmissionClient();
        var sender = new OrganizationInvitationEmailSender(
            client,
            new InvitationEmailSettings(
                "noreply@example.com",
                new Uri("https://licenses.example.com/invitations/accept")));
        var delivery = OrganizationInvitationDelivery.Restore(
            OrganizationInvitationDeliveryKind.Resent,
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "invitee@example.com",
            OrganizationRole.Admin,
            "secret&value?",
            ExpiresAt);

        await sender.SendAsync(deliveryId, delivery, CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(client.Submission!.DeliveryId, Is.EqualTo(deliveryId));
            Assert.That(client.Submission.FromAddress, Is.EqualTo("noreply@example.com"));
            Assert.That(client.Submission.ToAddress, Is.EqualTo("invitee@example.com"));
            Assert.That(client.Submission.Subject, Does.Contain("updated invitation"));
            Assert.That(
                client.Submission.TextBody,
                Does.Contain("secret%26value%3F"));
            Assert.That(
                client.Submission.HtmlBody,
                Does.Contain("secret%26value%3F"));
            Assert.That(client.Submission.TextBody, Does.Contain("administrator"));
            Assert.That(client.Submission.TextBody, Does.Contain("2026-09-08 12:30 UTC"));
            Assert.That(client.Submission.ToString(), Does.Not.Contain("secret"));
            Assert.That(client.Submission.ToString(), Does.Not.Contain("invitee"));
        });
    }

    [Test]
    public void SettingsRejectAcceptanceUrlWithExistingQuery()
    {
        Assert.That(
            () => new InvitationEmailSettings(
                "noreply@example.com",
                new Uri("https://licenses.example.com/accept?source=test")),
            Throws.ArgumentException);
    }

    [Test]
    public void SettingsRejectPlainHttpAcceptanceUrl()
    {
        Assert.That(
            () => new InvitationEmailSettings(
                "noreply@example.com",
                new Uri("http://licenses.example.com/accept")),
            Throws.ArgumentException.With.Message.Contains("HTTPS"));
    }

    [TestCase(0)]
    [TestCase(65536)]
    public void SmtpSettingsRejectUnsafePorts(int port)
    {
        Assert.That(
            () => new SmtpEmailSettings(
                "smtp.sendgrid.net",
                port,
                "apikey",
                "secret"),
            Throws.TypeOf<ArgumentOutOfRangeException>());
    }

    [Test]
    public async Task ExpiryIsRenderedInUtc()
    {
        var client = new RecordingEmailSubmissionClient();
        var sender = new OrganizationInvitationEmailSender(
            client,
            new InvitationEmailSettings(
                "noreply@example.com",
                new Uri("https://licenses.example.com/invitations/accept")));
        var delivery = OrganizationInvitationDelivery.Restore(
            OrganizationInvitationDeliveryKind.Created,
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "invitee@example.com",
            OrganizationRole.Member,
            "secret",
            new DateTimeOffset(2026, 9, 8, 14, 30, 0, TimeSpan.FromHours(2)));

        await sender.SendAsync(
            Guid.CreateVersion7(),
            delivery,
            CancellationToken.None);

        Assert.That(client.Submission!.TextBody, Does.Contain("2026-09-08 12:30 UTC"));
    }

    [Test]
    public void SmtpMessagePreservesContentAndAddsNonSecretDeliveryHeader()
    {
        var deliveryId = Guid.CreateVersion7();
        var submission = new InvitationEmailSubmission(
            deliveryId,
            "noreply@example.com",
            "invitee@example.com",
            "Invitation subject",
            "Text with a private link",
            "<p>HTML with a private link</p>");

        var message = SmtpEmailSubmissionClient.CreateMessage(submission);

        Assert.Multiple(() =>
        {
            Assert.That(message.From.Single().ToString(), Is.EqualTo(submission.FromAddress));
            Assert.That(message.To.Single().ToString(), Is.EqualTo(submission.ToAddress));
            Assert.That(message.Subject, Is.EqualTo(submission.Subject));
            Assert.That(message.Headers["X-Styrhous-Delivery-Id"], Is.EqualTo(deliveryId.ToString()));
            Assert.That(
                message.BodyParts.OfType<MimeKit.TextPart>()
                    .Single(part => part.IsPlain).Text,
                Is.EqualTo(submission.TextBody));
            Assert.That(
                message.BodyParts.OfType<MimeKit.TextPart>()
                    .Single(part => part.IsHtml).Text,
                Is.EqualTo(submission.HtmlBody));
        });
    }

    private sealed class RecordingEmailSubmissionClient : IEmailSubmissionClient
    {
        public InvitationEmailSubmission? Submission { get; private set; }

        public Task SendAsync(
            InvitationEmailSubmission submission,
            CancellationToken cancellationToken)
        {
            Submission = submission;
            return Task.CompletedTask;
        }
    }
}
