namespace Styrhous.Licensing.Application.Messaging;

public enum InfrastructureSmokeProbeClaimStatus
{
    Acquired,
    Unavailable,
    Busy,
}

internal enum InfrastructureSmokeProbeProcessingStatus
{
    Completed,
    Unavailable,
    Busy,
}
