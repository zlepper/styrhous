using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Desktop;

public sealed class DesktopDeviceAuthorizationService(
    PostgresDesktopDeviceAuthorizationStore store,
    TimeProvider timeProvider)
{

    public async Task<DesktopDeviceAuthorizationApproval> GetApprovalAsync(
        Guid userId,
        DesktopInstallation installation,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(installation);
        return await store.GetApprovalAsync(
            userId,
            installation,
            timeProvider.GetUtcNow(),
            cancellationToken);
    }

    public async Task<DesktopDeviceAuthorizationDecisionResult> ApproveAsync(
        Guid userId,
        Guid seatId,
        Guid authorizationId,
        DesktopInstallation installation,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(installation);

        var activation = await store.ApproveAsync(
            userId,
            seatId,
            authorizationId,
            installation,
            timeProvider.GetUtcNow(),
            Uuid7.Create(),
            cancellationToken);
        return DesktopDeviceAuthorizationDecisionResult.From(activation);
    }

}
