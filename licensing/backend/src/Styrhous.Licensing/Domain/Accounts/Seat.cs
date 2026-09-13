
namespace Styrhous.Licensing.Domain.Accounts;

public sealed class Seat
{
    public const int DefaultDeviceLimit = 3;

    private Seat()
    {
    }

    private Seat(
        Guid id,
        Guid billingAccountId,
        Guid assignedUserId,
        int deviceLimit,
        bool productAccessEnabled,
        DateTimeOffset createdAt)
    {
        Id = id;
        BillingAccountId = billingAccountId;
        AssignedUserId = assignedUserId;
        DeviceLimit = deviceLimit;
        ProductAccessEnabled = productAccessEnabled;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public Guid BillingAccountId { get; private set; }

    public Guid AssignedUserId { get; private set; }

    public int DeviceLimit { get; private set; }

    public bool ProductAccessEnabled { get; private set; }

    public DateTimeOffset CreatedAt { get; private set; }

    internal static Seat Assign(
        Guid billingAccountId,
        Guid assignedUserId,
        DateTimeOffset createdAt,
        bool productAccessEnabled = true)
    {

        return new Seat(
            Guid.CreateVersion7(),
            billingAccountId,
            assignedUserId,
            DefaultDeviceLimit,
            productAccessEnabled,
            createdAt.ToUniversalTime());
    }
}
