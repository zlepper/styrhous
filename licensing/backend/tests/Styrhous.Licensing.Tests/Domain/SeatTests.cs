using Styrhous.Licensing.Domain.Accounts;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class SeatTests
{
    [Test]
    public void SeatCanStartWithoutProductAccess()
    {
        var seat = Seat.Assign(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            DateTimeOffset.UtcNow,
            productAccessEnabled: false);

        Assert.That(seat.ProductAccessEnabled, Is.False);
    }
}
