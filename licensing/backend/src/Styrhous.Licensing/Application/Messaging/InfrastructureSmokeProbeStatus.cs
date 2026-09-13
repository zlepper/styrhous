namespace Styrhous.Licensing.Application.Messaging;

public sealed record InfrastructureSmokeProbeStatus(
    Guid ProbeId,
    Guid WorkId,
    bool IsProcessing,
    DateTimeOffset? DeliveredAt);
