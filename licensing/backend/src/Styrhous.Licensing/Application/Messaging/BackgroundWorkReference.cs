using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Messaging;

public enum BackgroundWorkKind
{
    OrganizationInvitationDelivery,
    InfrastructureSmokeProbe,
    BillingWebhook,
}

public sealed record BackgroundWorkReference
{
    private BackgroundWorkReference(
        BackgroundWorkKind kind,
        Guid workId,
        DateTimeOffset occurredAt)
    {

        Kind = kind;
        WorkId = workId;
        OccurredAt = occurredAt.ToUniversalTime();
    }

    public BackgroundWorkKind Kind { get; }

    public Guid WorkId { get; }

    public DateTimeOffset OccurredAt { get; }

    public static BackgroundWorkReference OrganizationInvitationDelivery(
        Guid workId,
        DateTimeOffset occurredAt)
    {
        return new(BackgroundWorkKind.OrganizationInvitationDelivery, workId, occurredAt);
    }

    public static BackgroundWorkReference BillingWebhook(
        Guid workId,
        DateTimeOffset occurredAt)
    {
        return new(BackgroundWorkKind.BillingWebhook, workId, occurredAt);
    }

    public static BackgroundWorkReference InfrastructureSmokeProbe(
        Guid workId,
        DateTimeOffset occurredAt)
    {
        return new(BackgroundWorkKind.InfrastructureSmokeProbe, workId, occurredAt);
    }
}
