using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationTests
{
    [Test]
    public void PendingInvitationNormalizesEmailAndExpiresAfterSevenDays()
    {
        var organizationId = Guid.CreateVersion7();
        var actorUserId = Guid.CreateVersion7();
        var createdAt = new DateTimeOffset(2026, 8, 31, 10, 15, 0, TimeSpan.Zero);
        var invitation = OrganizationInvitation.Create(
            organizationId,
            actorUserId,
            "  Member@Example.com  ",
            OrganizationRole.Member,
            new string('a', OrganizationInvitation.SecretHashLength),
            createdAt);

        Assert.Multiple(() =>
        {
            Assert.That(invitation.Id.Version, Is.EqualTo(7));
            Assert.That(invitation.OrganizationId, Is.EqualTo(organizationId));
            Assert.That(invitation.CreatedByUserId, Is.EqualTo(actorUserId));
            Assert.That(invitation.Email, Is.EqualTo("Member@Example.com"));
            Assert.That(invitation.NormalizedEmail, Is.EqualTo("MEMBER@EXAMPLE.COM"));
            Assert.That(invitation.Role, Is.EqualTo(OrganizationRole.Member));
            Assert.That(invitation.SecretHash, Is.EqualTo(new string('a', 64)));
            Assert.That(invitation.CreatedAt, Is.EqualTo(createdAt));
            Assert.That(invitation.LastSentAt, Is.EqualTo(createdAt));
            Assert.That(invitation.ExpiresAt, Is.EqualTo(createdAt.AddDays(7)));
            Assert.That(invitation.ReservedSeatCapacity, Is.Zero);
            Assert.That(invitation.AcceptedAt, Is.Null);
            Assert.That(invitation.CancelledAt, Is.Null);
        });
    }

    [Test]
    public void PendingInvitationRecordsAndCanRefreshPositiveReservedCapacity()
    {
        var invitation = CreateInvitation();

        invitation.ReserveSeatCapacity(5);
        invitation.ReserveSeatCapacity(3);

        Assert.That(invitation.ReservedSeatCapacity, Is.EqualTo(3));
    }

    [Test]
    public void SeatlessInvitationDoesNotReserveProductCapacity()
    {
        var invitation = OrganizationInvitation.Create(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "admin@example.com",
            OrganizationRole.Admin,
            new string('a', OrganizationInvitation.SecretHashLength),
            new DateTimeOffset(2026, 8, 31, 10, 15, 0, TimeSpan.Zero),
            assignProductSeat: false);

        Assert.Multiple(() =>
        {
            Assert.That(invitation.AssignProductSeat, Is.False);
            Assert.That(invitation.ReservedSeatCapacity, Is.Zero);
            Assert.That(
                () => invitation.ReserveSeatCapacity(5),
                Throws.TypeOf<InvalidOperationException>());
        });
    }

    [TestCase(0)]
    [TestCase(-1)]
    public void InvitationRejectsNonPositiveReservedCapacity(int seatCapacity)
    {
        var invitation = CreateInvitation();

        Assert.Throws<ArgumentOutOfRangeException>(
            () => invitation.ReserveSeatCapacity(seatCapacity));
        Assert.That(invitation.ReservedSeatCapacity, Is.Zero);
    }

    [TestCase(true)]
    [TestCase(false)]
    public void TerminalInvitationCannotChangeReservedCapacity(bool accepted)
    {
        var invitation = CreateInvitation();
        invitation.ReserveSeatCapacity(5);
        if (accepted)
        {
            invitation.TryAccept(Guid.CreateVersion7(), invitation.CreatedAt.AddDays(1));
        }
        else
        {
            invitation.TryCancel(invitation.CreatedAt.AddDays(1));
        }

        Assert.Throws<InvalidOperationException>(() => invitation.ReserveSeatCapacity(3));
        Assert.That(invitation.ReservedSeatCapacity, Is.EqualTo(5));
    }

    [TestCase(OrganizationRole.Owner)]
    [TestCase((OrganizationRole)999)]
    public void InvitationRejectsRolesThatCannotBeAssigned(OrganizationRole role)
    {
        Assert.Throws<ArgumentException>(
            () => CreateInvitation(role: role));
    }

    [TestCase("organizationId", "36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase("organizationId", TestIdentifiers.Version7WithNonRfcVariantText)]
    [TestCase("createdByUserId", "36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase("createdByUserId", TestIdentifiers.Version7WithNonRfcVariantText)]
    public void InvitationPreservesExistingRelationshipIdentifiers(string parameterName, string value)
    {
        var identifier = Guid.Parse(value);
        var invitation = OrganizationInvitation.Create(identifier, identifier,
            "member@example.com", OrganizationRole.Member,
            new string('a', OrganizationInvitation.SecretHashLength), DateTimeOffset.UtcNow);
        Assert.That(parameterName == "organizationId" ? invitation.OrganizationId : invitation.CreatedByUserId,
            Is.EqualTo(identifier));
    }

    [TestCase("")]
    [TestCase("not-hex")]
    [TestCase("gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg")]
    public void InvitationRejectsInvalidSecretHashes(string secretHash)
    {
        Assert.Throws<ArgumentException>(
            () => CreateInvitation(secretHash: secretHash));
    }

    [TestCase("")]
    [TestCase("not-an-email")]
    [TestCase("Name <member@example.com>")]
    public void InvitationRejectsInvalidEmailAddresses(string email)
    {
        Assert.Throws<ArgumentException>(
            () => CreateInvitation(email: email));
    }

    [Test]
    public void InvitationConvertsCreationTimeToUtcBeforeCalculatingExpiry()
    {
        var localTime = new DateTimeOffset(2026, 8, 31, 12, 15, 0, TimeSpan.FromHours(2));

        var invitation = CreateInvitation(createdAt: localTime);

        Assert.Multiple(() =>
        {
            Assert.That(invitation.CreatedAt, Is.EqualTo(localTime.ToUniversalTime()));
            Assert.That(invitation.ExpiresAt,
                Is.EqualTo(localTime.ToUniversalTime().AddDays(7)));
            Assert.That(invitation.CreatedAt.Offset, Is.EqualTo(TimeSpan.Zero));
            Assert.That(invitation.ExpiresAt.Offset, Is.EqualTo(TimeSpan.Zero));
        });
    }

    [Test]
    public void PendingInvitationCanBeCancelledExactlyOnce()
    {
        var invitation = CreateInvitation();
        var cancelledAt = invitation.CreatedAt.AddDays(1).ToOffset(TimeSpan.FromHours(2));

        var firstCancellation = invitation.TryCancel(cancelledAt);
        var repeatedCancellation = invitation.TryCancel(cancelledAt.AddHours(1));

        Assert.Multiple(() =>
        {
            Assert.That(firstCancellation, Is.True);
            Assert.That(repeatedCancellation, Is.False);
            Assert.That(invitation.CancelledAt, Is.EqualTo(cancelledAt.ToUniversalTime()));
            Assert.That(invitation.CancelledAt?.Offset, Is.EqualTo(TimeSpan.Zero));
        });
    }

    [Test]
    public void InvitationCannotBeCancelledBeforeItWasCreated()
    {
        var invitation = CreateInvitation();

        Assert.Throws<ArgumentOutOfRangeException>(
            () => invitation.TryCancel(invitation.CreatedAt.AddTicks(-1)));
        Assert.That(invitation.CancelledAt, Is.Null);
    }

    [Test]
    public void ExpiredInvitationCannotBeCancelled()
    {
        var invitation = CreateInvitation();

        var cancelled = invitation.TryCancel(invitation.ExpiresAt);

        Assert.Multiple(() =>
        {
            Assert.That(cancelled, Is.False);
            Assert.That(invitation.CancelledAt, Is.Null);
        });
    }

    [Test]
    public void ExpiredInvitationCanRotateItsSecretAndStartANewSevenDayWindow()
    {
        var invitation = CreateInvitation();
        var resentAt = invitation.ExpiresAt.AddDays(1).ToOffset(TimeSpan.FromHours(2));
        var replacementHash = new string('b', OrganizationInvitation.SecretHashLength);

        var resent = invitation.TryResend(replacementHash, resentAt);

        Assert.Multiple(() =>
        {
            Assert.That(resent, Is.True);
            Assert.That(invitation.SecretHash, Is.EqualTo(replacementHash));
            Assert.That(invitation.LastSentAt, Is.EqualTo(resentAt.ToUniversalTime()));
            Assert.That(invitation.LastSentAt.Offset, Is.EqualTo(TimeSpan.Zero));
            Assert.That(invitation.ExpiresAt,
                Is.EqualTo(resentAt.ToUniversalTime().AddDays(7)));
        });
    }

    [Test]
    public void CancelledInvitationCannotBeResent()
    {
        var invitation = CreateInvitation();
        invitation.TryCancel(invitation.CreatedAt.AddDays(1));

        var resent = invitation.TryResend(
            new string('b', OrganizationInvitation.SecretHashLength),
            invitation.CreatedAt.AddDays(2));

        Assert.Multiple(() =>
        {
            Assert.That(resent, Is.False);
            Assert.That(invitation.SecretHash, Is.EqualTo(new string('a', 64)));
            Assert.That(invitation.LastSentAt, Is.EqualTo(invitation.CreatedAt));
        });
    }

    [Test]
    public void InvitationCannotBeResentBeforeItWasCreated()
    {
        var invitation = CreateInvitation();

        Assert.Throws<ArgumentOutOfRangeException>(
            () => invitation.TryResend(
                new string('b', OrganizationInvitation.SecretHashLength),
                invitation.CreatedAt.AddTicks(-1)));
        Assert.That(invitation.SecretHash, Is.EqualTo(new string('a', 64)));
    }

    [Test]
    public void InvitationCannotMoveItsSendWindowBackwards()
    {
        var invitation = CreateInvitation();
        var firstResendAt = invitation.CreatedAt.AddDays(2);
        invitation.TryResend(
            new string('b', OrganizationInvitation.SecretHashLength),
            firstResendAt);

        Assert.Throws<ArgumentOutOfRangeException>(
            () => invitation.TryResend(
                new string('c', OrganizationInvitation.SecretHashLength),
                firstResendAt.AddTicks(-1)));
        Assert.Multiple(() =>
        {
            Assert.That(invitation.SecretHash, Is.EqualTo(new string('b', 64)));
            Assert.That(invitation.LastSentAt, Is.EqualTo(firstResendAt));
        });
    }

    [Test]
    public void InvitationCannotBeCancelledBeforeItsLastSend()
    {
        var invitation = CreateInvitation();
        var resentAt = invitation.CreatedAt.AddDays(2);
        invitation.TryResend(
            new string('b', OrganizationInvitation.SecretHashLength),
            resentAt);

        Assert.Throws<ArgumentOutOfRangeException>(
            () => invitation.TryCancel(resentAt.AddTicks(-1)));
        Assert.That(invitation.CancelledAt, Is.Null);
    }

    [Test]
    public void PendingInvitationCanBeAcceptedExactlyOnce()
    {
        var invitation = CreateInvitation();
        var userId = Guid.CreateVersion7();
        var acceptedAt = invitation.CreatedAt.AddDays(1).ToOffset(TimeSpan.FromHours(2));

        var firstAcceptance = invitation.TryAccept(userId, acceptedAt);
        var repeatedAcceptance = invitation.TryAccept(userId, acceptedAt.AddHours(1));

        Assert.Multiple(() =>
        {
            Assert.That(firstAcceptance, Is.True);
            Assert.That(repeatedAcceptance, Is.False);
            Assert.That(invitation.AcceptedByUserId, Is.EqualTo(userId));
            Assert.That(invitation.AcceptedAt, Is.EqualTo(acceptedAt.ToUniversalTime()));
            Assert.That(invitation.AcceptedAt?.Offset, Is.EqualTo(TimeSpan.Zero));
        });
    }

    [Test]
    public void InvitationCannotBeAcceptedBeforeItsLastSend()
    {
        var invitation = CreateInvitation();
        var resentAt = invitation.CreatedAt.AddDays(2);
        invitation.TryResend(
            new string('b', OrganizationInvitation.SecretHashLength),
            resentAt);

        Assert.Throws<ArgumentOutOfRangeException>(
            () => invitation.TryAccept(Guid.CreateVersion7(), resentAt.AddTicks(-1)));
        Assert.That(invitation.AcceptedAt, Is.Null);
    }

    [Test]
    public void InvitationCanBeAcceptedExactlyWhenItWasLastSent()
    {
        var invitation = CreateInvitation();
        var resentAt = invitation.CreatedAt.AddDays(2);
        invitation.TryResend(
            new string('b', OrganizationInvitation.SecretHashLength),
            resentAt);

        var accepted = invitation.TryAccept(Guid.CreateVersion7(), resentAt);

        Assert.Multiple(() =>
        {
            Assert.That(accepted, Is.True);
            Assert.That(invitation.AcceptedAt, Is.EqualTo(resentAt));
        });
    }

    [Test]
    public void InvitationCannotBeAcceptedAtItsExpiry()
    {
        var invitation = CreateInvitation();

        var accepted = invitation.TryAccept(Guid.CreateVersion7(), invitation.ExpiresAt);

        Assert.Multiple(() =>
        {
            Assert.That(accepted, Is.False);
            Assert.That(invitation.AcceptedAt, Is.Null);
            Assert.That(invitation.AcceptedByUserId, Is.Null);
        });
    }

    [Test]
    public void CancelledInvitationCannotBeAccepted()
    {
        var invitation = CreateInvitation();
        invitation.TryCancel(invitation.CreatedAt.AddDays(1));

        var accepted = invitation.TryAccept(
            Guid.CreateVersion7(),
            invitation.CreatedAt.AddDays(2));

        Assert.Multiple(() =>
        {
            Assert.That(accepted, Is.False);
            Assert.That(invitation.AcceptedAt, Is.Null);
            Assert.That(invitation.AcceptedByUserId, Is.Null);
        });
    }

    [TestCase("36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase(TestIdentifiers.Version7WithNonRfcVariantText)]
    public void InvitationAcceptsExistingUserIdentifier(string userId)
    {
        var invitation = CreateInvitation();
        var acceptedAt = invitation.CreatedAt.AddDays(1);
        Assert.That(invitation.TryAccept(Guid.Parse(userId), acceptedAt), Is.True);
        Assert.Multiple(() =>
        {
            Assert.That(invitation.AcceptedAt, Is.EqualTo(acceptedAt));
            Assert.That(invitation.AcceptedByUserId, Is.EqualTo(Guid.Parse(userId)));
        });
    }

    private static OrganizationInvitation CreateInvitation(
        OrganizationRole role = OrganizationRole.Member,
        string? secretHash = null,
        DateTimeOffset? createdAt = null,
        string email = "member@example.com")
    {
        return OrganizationInvitation.Create(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            email,
            role,
            secretHash ?? new string('a', OrganizationInvitation.SecretHashLength),
            createdAt ?? new DateTimeOffset(2026, 8, 31, 10, 15, 0, TimeSpan.Zero));
    }
}
