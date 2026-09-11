using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class MessagingIdentifierTests
{
    private static readonly DateTimeOffset ObservedAt =
        new(2026, 9, 1, 10, 0, 0, TimeSpan.Zero);

    [Test]
    public void OutboxMessagePreservesExistingCorrelationIdentifiers()
    {
        var correlationId = Guid.NewGuid();
        var message = OutboxMessage.Enqueue(correlationId, Guid.CreateVersion7(),
            OutboxMessageTypes.OrganizationInvitationDelivery, "protected-payload", ObservedAt);
        Assert.That(message.CorrelationId, Is.EqualTo(correlationId));
    }

    [Test]
    public void OutboxMessagePreservesExistingSubjectIdentifiers()
    {
        var subjectId = Guid.NewGuid();
        var message = OutboxMessage.Enqueue(Guid.CreateVersion7(), subjectId,
            OutboxMessageTypes.OrganizationInvitationDelivery, "protected-payload", ObservedAt);
        Assert.That(message.SubjectId, Is.EqualTo(subjectId));
    }

    [Test]
    public void OutboxMessageEnforcesTextBoundaries()
    {
        var exactMaximum = OutboxMessage.Enqueue(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            new string('m', OutboxMessage.MaximumMessageTypeLength),
            new string('p', OutboxMessage.MaximumProtectedPayloadLength),
            ObservedAt);
        var invalidInputs = new (string MessageType, string Payload, string ParameterName)[]
        {
            (" ", "payload", "messageType"),
            ("type", " ", "protectedPayload"),
            (new string('m', OutboxMessage.MaximumMessageTypeLength + 1), "payload", "messageType"),
            ("type", new string('p', OutboxMessage.MaximumProtectedPayloadLength + 1), "protectedPayload"),
        };

        Assert.Multiple(() =>
        {
            Assert.That(
                exactMaximum.MessageType,
                Has.Length.EqualTo(OutboxMessage.MaximumMessageTypeLength));
            Assert.That(
                exactMaximum.ProtectedPayload,
                Has.Length.EqualTo(OutboxMessage.MaximumProtectedPayloadLength));
        });
        foreach (var (messageType, payload, parameterName) in invalidInputs)
        {
            var exception = Assert.Throws<ArgumentException>(
                () => OutboxMessage.Enqueue(
                    Guid.CreateVersion7(),
                    Guid.CreateVersion7(),
                    messageType,
                    payload,
                    ObservedAt));
            Assert.That(exception!.ParamName, Is.EqualTo(parameterName));
        }
    }

    [TestCase(0)]
    [TestCase(-1)]
    public void OutboxMessageRequiresExpiryAfterOccurrence(int secondsAfterOccurrence)
    {
        var exception = Assert.Throws<ArgumentOutOfRangeException>(
            () => OutboxMessage.Enqueue(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                OutboxMessageTypes.OrganizationInvitationDelivery,
                "protected-payload",
                ObservedAt,
                ObservedAt.AddSeconds(secondsAfterOccurrence)));

        Assert.That(exception!.ParamName, Is.EqualTo("notAfter"));
    }

    [Test]
    public void OutboxProcessingLeaseSupportsRetryAndIdempotentCompletion()
    {
        var message = CreateExpiringOutbox();
        var firstLeaseId = Guid.CreateVersion7();
        var secondLeaseId = Guid.CreateVersion7();

        Assert.Multiple(() =>
        {
            Assert.That(
                message.TryAcquireProcessingLease(
                    firstLeaseId,
                    ObservedAt.AddMinutes(1),
                    ObservedAt.AddMinutes(6)),
                Is.True);
            Assert.That(message.ProcessingLeaseId, Is.EqualTo(firstLeaseId));
            Assert.That(
                message.ProcessingLeaseExpiresAt,
                Is.EqualTo(ObservedAt.AddMinutes(6)));
            Assert.That(message.ProcessingAttemptCount, Is.EqualTo(1));
            Assert.That(
                message.TryAcquireProcessingLease(
                    secondLeaseId,
                    ObservedAt.AddMinutes(2),
                    ObservedAt.AddMinutes(7)),
                Is.False);
            Assert.That(
                message.TryAcquireProcessingLease(
                    secondLeaseId,
                    ObservedAt.AddMinutes(6),
                    ObservedAt.AddMinutes(11)),
                Is.True);
            Assert.That(message.ProcessingAttemptCount, Is.EqualTo(2));
            Assert.That(message.TryMarkDelivered(firstLeaseId, ObservedAt.AddMinutes(7)), Is.False);
            Assert.That(message.TryMarkDelivered(secondLeaseId, ObservedAt.AddMinutes(7)), Is.True);
            Assert.That(message.DeliveredAt, Is.EqualTo(ObservedAt.AddMinutes(7)));
            Assert.That(message.ProcessingLeaseId, Is.Null);
            Assert.That(message.ProcessingLeaseExpiresAt, Is.Null);
            Assert.That(
                message.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    ObservedAt.AddMinutes(8),
                    ObservedAt.AddMinutes(13)),
                Is.False);
        });
    }

    [Test]
    public void OutboxProcessingLeaseCanBeReleasedForRetry()
    {
        var message = CreateExpiringOutbox();
        var leaseId = Guid.CreateVersion7();
        Assert.That(
            message.TryAcquireProcessingLease(
                leaseId,
                ObservedAt.AddMinutes(1),
                ObservedAt.AddMinutes(6)),
            Is.True);

        Assert.Multiple(() =>
        {
            Assert.That(message.TryReleaseProcessingLease(Guid.CreateVersion7()), Is.False);
            Assert.That(message.TryReleaseProcessingLease(leaseId), Is.True);
            Assert.That(message.ProcessingLeaseId, Is.Null);
            Assert.That(message.ProcessingLeaseExpiresAt, Is.Null);
            Assert.That(message.TryReleaseProcessingLease(leaseId), Is.False);
        });
    }

    [Test]
    public void OutboxProcessingLeasePreservesAnExistingIdentifier()
    {
        var message = CreateExpiringOutbox();
        var leaseId = Guid.NewGuid();
        Assert.That(message.TryAcquireProcessingLease(leaseId,
            ObservedAt.AddMinutes(1), ObservedAt.AddMinutes(6)), Is.True);
        Assert.That(message.ProcessingLeaseId, Is.EqualTo(leaseId));
    }

    [Test]
    public void OutboxProcessingLeaseValidatesWindow()
    {
        var message = CreateExpiringOutbox();

        Assert.Multiple(() =>
        {
            Assert.That(
                () => message.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    ObservedAt.AddSeconds(-1),
                    ObservedAt.AddMinutes(5)),
                Throws.TypeOf<ArgumentOutOfRangeException>()
                    .With.Property("ParamName").EqualTo("acquiredAt"));
            Assert.That(
                () => message.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    ObservedAt.AddMinutes(1),
                    ObservedAt.AddMinutes(1)),
                Throws.TypeOf<ArgumentOutOfRangeException>()
                    .With.Property("ParamName").EqualTo("expiresAt"));
        });
    }

    [Test]
    public void OutboxCannotBeLeasedAtExpiryOrAfterDiscard()
    {
        var expired = CreateExpiringOutbox();
        var discarded = CreateExpiringOutbox();
        Assert.That(
            discarded.TryDiscard(
                OutboxDiscardReason.InvitationCancelled,
                ObservedAt.AddMinutes(1)),
            Is.True);

        Assert.Multiple(() =>
        {
            Assert.That(
                expired.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    ObservedAt.AddHours(1),
                    ObservedAt.AddHours(1).AddMinutes(5)),
                Is.False);
            Assert.That(
                discarded.TryAcquireProcessingLease(
                    Guid.CreateVersion7(),
                    ObservedAt.AddMinutes(2),
                    ObservedAt.AddMinutes(7)),
                Is.False);
        });
    }

    [Test]
    public void OutboxCannotBeMarkedDeliveredAtItsDeadline()
    {
        var message = CreateExpiringOutbox();
        var leaseId = Guid.CreateVersion7();
        Assert.That(
            message.TryAcquireProcessingLease(
                leaseId,
                ObservedAt.AddMinutes(1),
                ObservedAt.AddHours(1)),
            Is.True);

        Assert.Multiple(() =>
        {
            Assert.That(message.TryMarkDelivered(leaseId, ObservedAt.AddHours(1)), Is.False);
            Assert.That(message.DeliveredAt, Is.Null);
            Assert.That(message.ProcessingLeaseId, Is.EqualTo(leaseId));
        });
    }

    [Test]
    public void DiscardingPendingDeliveryInvalidatesItsLease()
    {
        var message = CreateExpiringOutbox();
        var leaseId = Guid.CreateVersion7();
        Assert.That(
            message.TryAcquireProcessingLease(
                leaseId,
                ObservedAt.AddMinutes(2),
                ObservedAt.AddMinutes(7)),
            Is.True);

        Assert.Multiple(() =>
        {
            Assert.That(
                message.TryDiscard(
                    OutboxDiscardReason.InvitationCancelled,
                    ObservedAt.AddMinutes(3)),
                Is.True);
            Assert.That(message.ProcessingLeaseId, Is.Null);
            Assert.That(message.ProcessingLeaseExpiresAt, Is.Null);
            Assert.That(message.TryMarkDelivered(leaseId, ObservedAt.AddMinutes(4)), Is.False);
        });
    }

    [Test]
    public void InvitationDeliveryPreservesExistingRelationshipIdentifiers()
    {
        var invitationId = Guid.NewGuid();
        var organizationId = Guid.NewGuid();
        var delivery = OrganizationInvitationDelivery.Restore(
            OrganizationInvitationDeliveryKind.Created, invitationId, organizationId,
            "invitee@example.com", OrganizationRole.Member, "delivery-secret", ObservedAt);
        Assert.Multiple(() =>
        {
            Assert.That(delivery.InvitationId, Is.EqualTo(invitationId));
            Assert.That(delivery.OrganizationId, Is.EqualTo(organizationId));
        });
    }

    [Test]
    public void InvitationDeliveryRequiresThePersistedInvitationSecret()
    {
        var persistedSecret = new OrganizationInvitationSecret("persisted-secret");
        var invitation = OrganizationInvitation.Create(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "invitee@example.com",
            OrganizationRole.Member,
            persistedSecret.Hash,
            ObservedAt);

        var exception = Assert.Throws<ArgumentException>(
            () => OrganizationInvitationDelivery.From(
                OrganizationInvitationDeliveryKind.Created,
                invitation,
                new OrganizationInvitationSecret("different-secret")));

        Assert.That(exception!.ParamName, Is.EqualTo("secret"));
    }

    private static OutboxMessage CreateExpiringOutbox()
    {
        return OutboxMessage.Enqueue(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            OutboxMessageTypes.OrganizationInvitationDelivery,
            "protected-payload",
            ObservedAt,
            ObservedAt.AddHours(1));
    }
}
