using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Domain.Trials;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
public sealed class RelationshipIdentifierTests
{
    [TestCase("36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase(TestIdentifiers.Version7WithNonRfcVariantText)]
    public void ExistingRelationshipIdentifiersArePreservedWithoutVersionRestrictions(string value)
    {
        var identifier = Guid.Parse(value);
        var observedAt = DateTimeOffset.UtcNow;
        var verifiedIdentity = VerifiedExternalIdentity.Create("github", "subject", "person@example.com");
        var billingAccount = BillingAccount.CreatePersonal(identifier, observedAt);
        var seat = Seat.Assign(identifier, identifier, observedAt);
        var trial = Trial.Start(identifier, identifier, observedAt);
        var identity = ExternalIdentity.Create(identifier, verifiedIdentity, observedAt);
        var emailClaim = VerifiedEmailClaim.Create(identifier, verifiedIdentity, observedAt);
        var installation = DesktopInstallation.Create(identifier, "Laptop", "linux", "x86_64", "1.2.3");
        var activation = DeviceActivation.Activate(identifier, identifier, installation, observedAt);
        var audit = AuditRecord.Create(identifier, identifier, AuditAction.DeviceActivated,
            AuditTargetType.DeviceActivation, identifier, observedAt);

        Assert.Multiple(() =>
        {
            Assert.That(billingAccount.PersonalOwnerUserId, Is.EqualTo(identifier));
            Assert.That(seat.BillingAccountId, Is.EqualTo(identifier));
            Assert.That(seat.AssignedUserId, Is.EqualTo(identifier));
            Assert.That(trial.OriginatingUserId, Is.EqualTo(identifier));
            Assert.That(trial.BillingAccountId, Is.EqualTo(identifier));
            Assert.That(identity.UserId, Is.EqualTo(identifier));
            Assert.That(emailClaim.UserId, Is.EqualTo(identifier));
            Assert.That(activation.SeatId, Is.EqualTo(identifier));
            Assert.That(activation.UserId, Is.EqualTo(identifier));
            Assert.That(activation.InstallationId, Is.EqualTo(identifier));
            Assert.That(DeviceActivationPolicy.Decide([activation], identifier, 1, observedAt).Kind,
                Is.EqualTo(DeviceActivationDecisionKind.AlreadyActive));
            Assert.That(audit.CorrelationId, Is.EqualTo(identifier));
            Assert.That(audit.ActorUserId, Is.EqualTo(identifier));
            Assert.That(audit.TargetId, Is.EqualTo(identifier));
        });
    }

    [TestCase("36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase(TestIdentifiers.Version7WithNonRfcVariantText)]
    public void TrialTransferPreservesAnExistingOrganizationIdentifier(string value)
    {
        var identifier = Guid.Parse(value);
        var trial = Trial.Start(Guid.CreateVersion7(), Guid.CreateVersion7(), DateTimeOffset.UtcNow);
        var transferredAt = trial.StartedAt.AddDays(1);
        Assert.That(trial.TryTransferToFirstOrganization(identifier, transferredAt), Is.True);
        Assert.Multiple(() =>
        {
            Assert.That(trial.BillingAccountId, Is.EqualTo(identifier));
            Assert.That(trial.TransferredAt, Is.EqualTo(transferredAt));
        });
    }
}
