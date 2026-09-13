using Styrhous.Licensing.Application.Entitlements;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
public sealed class CommercialSeatFundingPolicyTests
{
    [Test]
    public void FundingOrderIsExplicitAndIndependentOfCandidateInputOrder()
    {
        var billingAccountId = Guid.CreateVersion7();
        var createdAt = new DateTimeOffset(2026, 8, 31, 10, 0, 0, TimeSpan.Zero);
        var founderSeatId = Guid.Parse("01917f8b-6000-7000-8000-000000000000");
        var earlierTiedSeatId = Guid.Parse("01917f8b-6000-7000-8000-000000000001");
        var laterTiedSeatId = Guid.Parse("01917f8b-6000-7000-8000-000000000002");
        var currentOwnerSeatId = Guid.Parse("01917f8b-6000-7000-8000-000000000003");
        CommercialSeatCandidate[] candidates =
        [
            new(laterTiedSeatId, billingAccountId, createdAt.AddDays(1), false),
            new(currentOwnerSeatId, billingAccountId, createdAt.AddDays(2), true),
            new(earlierTiedSeatId, billingAccountId, createdAt.AddDays(1), false),
            new(founderSeatId, billingAccountId, createdAt, false),
        ];

        var funded = CommercialSeatFundingPolicy.Resolve(
            candidates,
            new Dictionary<Guid, int> { [billingAccountId] = 3 });

        Assert.That(
            funded,
            Is.EquivalentTo(
                new[]
                {
                    currentOwnerSeatId,
                    founderSeatId,
                    earlierTiedSeatId,
                }));
    }

    [Test]
    public void CandidateWithoutAProjectedQuantityIsNotFunded()
    {
        var candidate = new CommercialSeatCandidate(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            DateTimeOffset.UtcNow,
            IsOrganizationOwner: false);

        var funded = CommercialSeatFundingPolicy.Resolve(
            new[] { candidate },
            new Dictionary<Guid, int>());

        Assert.That(funded, Is.Empty);
    }
}
