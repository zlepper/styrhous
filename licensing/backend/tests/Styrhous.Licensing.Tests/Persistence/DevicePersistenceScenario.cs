using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

internal static class DevicePersistenceScenario
{
    public static Task<DeviceActivationResult> ActivateAsync(
        PostgresTestDatabase database,
        SignupResult signup,
        int installationNumber,
        DateTimeOffset observedAt)
    {
        return ActivateAsync(database, signup, CreateInstallation(installationNumber), observedAt);
    }

    public static Task<DeviceActivationResult> ActivateAsync(
        PostgresTestDatabase database,
        SignupResult signup,
        DesktopInstallation installation,
        DateTimeOffset observedAt)
    {
        return ActivateAsync(
            database,
            signup.UserId,
            signup.SeatId,
            installation,
            observedAt);
    }

    public static Task<DeviceActivationResult> ActivateAsync(
        PostgresTestDatabase database,
        Guid userId,
        Guid seatId,
        int installationNumber,
        DateTimeOffset observedAt)
    {
        return ActivateAsync(
            database,
            userId,
            seatId,
            CreateInstallation(installationNumber),
            observedAt);
    }

    public static Task<DeviceActivationResult> ActivateAsync(
        PostgresTestDatabase database,
        Guid userId,
        Guid seatId,
        DesktopInstallation installation,
        DateTimeOffset observedAt)
    {
        return DevicePersistenceScenario.ActivateAsync(database.CreateContextFactory(), observedAt, userId, seatId, installation);
    }

    public static Task<DeviceActivationResult> ActivateAsync(
        IDbContextFactory<LicensingDbContext> contextFactory,
        DateTimeOffset observedAt,
        Guid userId,
        Guid seatId,
        DesktopInstallation installation,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(installation);
        var correlationId = Guid.CreateVersion7();
        return EfConcurrencyRetry.ExecuteAsync(async () =>
        {
            await using var context = await contextFactory.CreateDbContextAsync(cancellationToken);
            await using var transaction = await context.Database.BeginTransactionAsync(
                IsolationLevel.Serializable, cancellationToken);
            var result = await PostgresDeviceActivationOperation.ExecuteAsync(
                context, userId, seatId, installation, observedAt, correlationId, cancellationToken);
            if (result.Status == DeviceActivationStatus.Activated)
            {
                await context.SaveChangesAsync(cancellationToken);
                await transaction.CommitAsync(cancellationToken);
            }
            return result;
        });
    }

    public static DesktopInstallation CreateInstallation(int number)
    {
        return DesktopInstallation.Create(
            Guid.CreateVersion7(),
            $"Device {number}",
            "linux",
            "x86_64",
            "1.2.3");
    }
}
