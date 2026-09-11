using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Signups;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
public sealed class SignupRegistrationTests
{
    [Test]
    public void StartingARegistrationCreatesAnImmediateThirtyDayPersonalTrial()
    {
        var observedAt = new DateTimeOffset(2026, 8, 30, 12, 34, 56, TimeSpan.FromHours(2));
        var identity = VerifiedExternalIdentity.Create("github", "provider-subject", "person@example.com");

        var registration = SignupRegistration.Start(identity, observedAt);

        Assert.Multiple(() =>
        {
            Assert.That(registration.ExternalIdentity.Provider, Is.EqualTo("github"));
            Assert.That(registration.ExternalIdentity.Subject, Is.EqualTo("provider-subject"));
            Assert.That(registration.ExternalIdentity.UserId, Is.EqualTo(registration.User.Id));
            Assert.That(registration.VerifiedEmailClaim.UserId, Is.EqualTo(registration.User.Id));
            Assert.That(
                registration.VerifiedEmailClaim.NormalizedEmail,
                Is.EqualTo("PERSON@EXAMPLE.COM"));
            Assert.That(registration.User.VerifiedEmail, Is.EqualTo("person@example.com"));
            Assert.That(registration.User.NormalizedEmail, Is.EqualTo("PERSON@EXAMPLE.COM"));
            Assert.That(registration.User.CreatedAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(registration.BillingAccount.Kind, Is.EqualTo(BillingAccountKind.Personal));
            Assert.That(
                registration.BillingAccount.PersonalOwnerUserId,
                Is.EqualTo(registration.User.Id));
            Assert.That(registration.Seat.BillingAccountId, Is.EqualTo(registration.BillingAccount.Id));
            Assert.That(registration.Seat.AssignedUserId, Is.EqualTo(registration.User.Id));
            Assert.That(registration.Trial.OriginatingUserId, Is.EqualTo(registration.User.Id));
            Assert.That(registration.Trial.BillingAccountId, Is.EqualTo(registration.BillingAccount.Id));
            Assert.That(registration.Trial.StartedAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(registration.Trial.EndsAt, Is.EqualTo(observedAt.ToUniversalTime().AddDays(30)));
            Assert.That(registration.Trial.TransferredAt, Is.Null);
        });

        var ownedIds = new[]
        {
            registration.User.Id,
            registration.ExternalIdentity.Id,
            registration.VerifiedEmailClaim.Id,
            registration.BillingAccount.Id,
            registration.Seat.Id,
            registration.Trial.Id,
        };

        Assert.Multiple(() =>
        {
            Assert.That(ownedIds, Is.Unique);
            Assert.That(ownedIds, Has.All.Property(nameof(Guid.Version)).EqualTo(7));
        });
    }

    [TestCase("", "subject", "person@example.com")]
    [TestCase("github", "", "person@example.com")]
    [TestCase("github", "subject", "")]
    public void ExternalIdentityRequiresProviderSubjectAndVerifiedEmail(
        string provider,
        string subject,
        string verifiedEmail)
    {
        Assert.That(
            () => VerifiedExternalIdentity.Create(provider, subject, verifiedEmail),
            Throws.TypeOf<ArgumentException>());
    }

    [Test]
    public void ExternalIdentityAcceptsValuesAtThePersistenceLimits()
    {
        var maximumLengthEmail = $"{new string('e', 64)}@{string.Join(
            ".",
            Enumerable.Repeat(new string('d', 63), 4))}";
        var identity = VerifiedExternalIdentity.Create(
            new string('p', VerifiedExternalIdentity.MaximumProviderLength),
            new string('s', VerifiedExternalIdentity.MaximumSubjectLength),
            maximumLengthEmail);

        Assert.Multiple(() =>
        {
            Assert.That(identity.Provider, Has.Length.EqualTo(64));
            Assert.That(identity.Subject, Has.Length.EqualTo(512));
            Assert.That(identity.VerifiedEmail, Has.Length.EqualTo(320));
            Assert.That(identity.NormalizedEmail, Has.Length.EqualTo(320));
        });
    }

    [TestCase("provider")]
    [TestCase("subject")]
    [TestCase("email")]
    public void ExternalIdentityRejectsValuesBeyondThePersistenceLimits(string field)
    {
        var provider = field == "provider" ? new string('p', 65) : "github";
        var subject = field == "subject" ? new string('s', 513) : "subject";
        var email = field == "email" ? new string('e', 321) : "person@example.com";

        Assert.That(
            () => VerifiedExternalIdentity.Create(provider, subject, email),
            Throws.TypeOf<ArgumentException>());
    }

}
