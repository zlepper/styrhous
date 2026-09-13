using System.Security.Claims;
using Microsoft.AspNetCore.Http.HttpResults;
using Styrhous.Licensing.Api.Antiforgery;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Application.Devices;

namespace Styrhous.Licensing.Api.Devices;

public static class DeviceEndpoints
{
    public static IEndpointRouteBuilder MapDeviceEndpoints(this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapGet("/api/seats/{seatId}/devices", ListAsync)
            .RequireAuthorization();
        endpoints.MapDelete("/api/device-activations/{activationId}", RevokeAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        return endpoints;
    }

    private static async Task<IResult> ListAsync(
        ClaimsPrincipal principal,
        string seatId,
        DeviceListingService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(seatId, out var parsedSeatId))
        {
            return NotFound();
        }

        var result = await service.ListActiveAsync(userId, parsedSeatId, cancellationToken);
        return result.Status switch
        {
            DeviceListingStatus.Listed => TypedResults.Ok(DeviceListResponse.From(result)),
            DeviceListingStatus.SeatNotFound => NotFound(),
            _ => throw new InvalidOperationException(
                $"Unsupported device listing status: {result.Status}."),
        };
    }

    private static NotFound<DeviceListingErrorResponse> NotFound()
    {
        return TypedResults.NotFound(DeviceListingErrorResponse.SeatNotFound());
    }

    private static async Task<IResult> RevokeAsync(
        ClaimsPrincipal principal,
        string activationId,
        DeviceRevocationService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(activationId, out var parsedActivationId))
        {
            return DeviceNotActive();
        }

        var result = await service.RevokeAsync(
            userId,
            parsedActivationId,
            cancellationToken);
        return result.Status switch
        {
            DeviceRevocationStatus.Revoked =>
                TypedResults.Ok(DeviceRevocationResponse.From(result)),
            DeviceRevocationStatus.DeviceNotActive => DeviceNotActive(),
            _ => throw new InvalidOperationException(
                $"Unsupported device revocation status: {result.Status}."),
        };
    }

    private static NotFound<DeviceRevocationErrorResponse> DeviceNotActive()
    {
        return TypedResults.NotFound(DeviceRevocationErrorResponse.DeviceNotActive());
    }
}
