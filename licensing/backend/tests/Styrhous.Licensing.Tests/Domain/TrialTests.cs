using Styrhous.Licensing.Domain.Trials;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class TrialTests
{
    private static readonly DateTimeOffset StartedAt =
        new(2026, 9, 1, 8, 0, 0, TimeSpan.Zero);

    [Test]
    public void PaidConversionShortensTrialWithoutChangingItsOriginalWindow()
    {
        var trial = Trial.Start(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            StartedAt);
        var originalEndsAt = trial.EndsAt;
        var terminatedAt = StartedAt.AddDays(4);

        var terminated = trial.TryTerminate(terminatedAt);
        var repeated = trial.TryTerminate(terminatedAt.AddDays(1));

        Assert.Multiple(() =>
        {
            Assert.That(terminated, Is.True);
            Assert.That(repeated, Is.False);
            Assert.That(trial.StartedAt, Is.EqualTo(StartedAt));
            Assert.That(trial.EndsAt, Is.EqualTo(originalEndsAt));
            Assert.That(trial.TerminatedAt, Is.EqualTo(terminatedAt));
            Assert.That(trial.EffectiveEndsAt, Is.EqualTo(terminatedAt));
        });
    }

    [Test]
    public void NaturalExpiryCannotBeExtendedOrRewrittenAsTermination()
    {
        var trial = Trial.Start(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            StartedAt);

        Assert.Multiple(() =>
        {
            Assert.That(trial.TryTerminate(trial.EndsAt), Is.False);
            Assert.That(trial.TryTerminate(trial.EndsAt.AddDays(1)), Is.False);
            Assert.That(trial.TerminatedAt, Is.Null);
            Assert.That(trial.EffectiveEndsAt, Is.EqualTo(trial.EndsAt));
        });
    }

    [Test]
    public void TerminationCannotPredateTrial()
    {
        var trial = Trial.Start(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            StartedAt);

        Assert.That(
            () => trial.TryTerminate(StartedAt.AddTicks(-1)),
            Throws.TypeOf<ArgumentOutOfRangeException>());
    }

    [Test]
    public void TerminatedTrialCannotLaterTransferToAnOrganization()
    {
        var personalBillingAccountId = Guid.CreateVersion7();
        var trial = Trial.Start(
            Guid.CreateVersion7(),
            personalBillingAccountId,
            StartedAt);
        Assert.That(trial.TryTerminate(StartedAt.AddDays(1)), Is.True);

        var transferred = trial.TryTransferToFirstOrganization(
            Guid.CreateVersion7(),
            StartedAt.AddDays(2));

        Assert.Multiple(() =>
        {
            Assert.That(transferred, Is.False);
            Assert.That(trial.BillingAccountId, Is.EqualTo(personalBillingAccountId));
            Assert.That(trial.TransferredAt, Is.Null);
        });
    }
}
