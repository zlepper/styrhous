using Styrhous.Licensing.Infrastructure.Messaging;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BackgroundWorkMessageTests
{
    [Test]
    public void ContractsPreserveExistingIdentifierVersions()
    {
        var invitationId = Guid.NewGuid();
        var billingId = Guid.NewGuid();
        var smokeProbeId = Guid.NewGuid();

        Assert.Multiple(() =>
        {
            Assert.That(
                new OrganizationInvitationDeliveryMessage(invitationId).WorkId,
                Is.EqualTo(invitationId));
            Assert.That(
                new BillingWebhookProcessingMessage(billingId).WorkId,
                Is.EqualTo(billingId));
            Assert.That(
                new InfrastructureSmokeProbeMessage(smokeProbeId).WorkId,
                Is.EqualTo(smokeProbeId));
        });
    }
}
