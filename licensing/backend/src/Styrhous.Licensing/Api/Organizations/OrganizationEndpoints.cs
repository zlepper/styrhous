using System.Security.Claims;
using Microsoft.AspNetCore.Http.HttpResults;
using Styrhous.Licensing.Api.Antiforgery;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Api.Organizations;

public static class OrganizationEndpoints
{
    public static IEndpointRouteBuilder MapOrganizationEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapGet("/api/organizations", ListAsync)
            .RequireAuthorization();
        endpoints.MapGet(
                "/api/organizations/{organizationId}/members",
                ListMembersAsync)
            .RequireAuthorization();
        endpoints.MapDelete(
                "/api/organizations/{organizationId}/members/{membershipId}",
                RemoveMemberAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPatch(
                "/api/organizations/{organizationId}/members/{membershipId}/role",
                ChangeMemberRoleAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPatch(
                "/api/organizations/{organizationId}/members/{membershipId}/seat",
                ChangeMemberSeatAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPost(
                "/api/organizations/{organizationId}/members/{membershipId}/transfer-ownership",
                TransferOwnershipAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapGet(
                "/api/organizations/{organizationId}/invitations",
                ListInvitationsAsync)
            .RequireAuthorization();
        endpoints.MapPost("/api/organizations", CreateAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPost(
                "/api/organizations/{organizationId}/invitations",
                CreateInvitationAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapDelete(
                "/api/organizations/{organizationId}/invitations/{invitationId}",
                CancelInvitationAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPost(
                "/api/organizations/{organizationId}/invitations/{invitationId}/resend",
                ResendInvitationAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPost("/api/invitations/accept", AcceptInvitationAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        return endpoints;
    }

    private static async Task<IResult> ListAsync(
        ClaimsPrincipal principal,
        OrganizationListingService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        try
        {
            var organizations = await service.ListForUserAsync(userId, cancellationToken);
            return TypedResults.Ok(OrganizationListResponse.From(organizations));
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> CreateAsync(
        ClaimsPrincipal principal,
        OrganizationCreationRequest request,
        OrganizationCreationService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        try
        {
            var result = await service.CreateAsync(
                actorUserId,
                request.Name!,
                cancellationToken);
            return TypedResults.Created(
                $"/api/organizations/{result.OrganizationId}",
                OrganizationCreationResponse.From(result));
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }

    }

    private static async Task<IResult> CreateInvitationAsync(
        string organizationId,
        ClaimsPrincipal principal,
        OrganizationInvitationCreationRequest request,
        OrganizationInvitationCreationService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationInvitationReasonCodes.OrganizationNotFound);
        }

        _ = OrganizationRoleNames.TryParseNonOwner(request.Role, out var role);

        try
        {
            var result = await service.CreateAsync(
                actorUserId,
                parsedOrganizationId,
                request.Email!,
                role,
                request.AssignProductSeat ?? true,
                cancellationToken);
            if (result is OrganizationInvitationCreationResult.Success success)
            {
                return TypedResults.Created(
                    $"/api/organizations/{parsedOrganizationId}/invitations/{success.InvitationId}",
                    OrganizationInvitationCreationResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationInvitationCreationStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationInvitationReasonCodes.OrganizationNotFound),
                OrganizationInvitationCreationStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationInvitationReasonCodes.InsufficientPermission),
                OrganizationInvitationCreationStatus.AlreadyMember =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.AlreadyMember),
                OrganizationInvitationCreationStatus.InvitationAlreadyPending =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.InvitationAlreadyPending),
                OrganizationInvitationCreationStatus.NoActiveSeatCapacity =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.NoActiveSeatCapacity),
                OrganizationInvitationCreationStatus.SeatCapacityReached =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.SeatCapacityReached),
                OrganizationInvitationCreationStatus.Created =>
                    throw new InvalidOperationException(
                        "A successful invitation result did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported invitation creation status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> CancelInvitationAsync(
        string organizationId,
        string invitationId,
        ClaimsPrincipal principal,
        OrganizationInvitationCancellationService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationInvitationReasonCodes.OrganizationNotFound);
        }

        if (!Guid.TryParse(invitationId, out var parsedInvitationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationInvitationReasonCodes.InvitationNotFound);
        }

        try
        {
            var result = await service.CancelAsync(
                actorUserId,
                parsedOrganizationId,
                parsedInvitationId,
                cancellationToken);
            if (result is OrganizationInvitationCancellationResult.Success success)
            {
                return TypedResults.Ok(
                    OrganizationInvitationCancellationResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationInvitationCancellationStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationInvitationReasonCodes.OrganizationNotFound),
                OrganizationInvitationCancellationStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationInvitationReasonCodes.InsufficientPermission),
                OrganizationInvitationCancellationStatus.InvitationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationInvitationReasonCodes.InvitationNotFound),
                OrganizationInvitationCancellationStatus.Superseded =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.CancellationSuperseded),
                OrganizationInvitationCancellationStatus.Cancelled =>
                    throw new InvalidOperationException(
                        "A successful cancellation result did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported invitation cancellation status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static JsonHttpResult<OrganizationErrorResponse>
        OrganizationError(int statusCode, string reasonCode)
    {
        return TypedResults.Json(
            new OrganizationErrorResponse(reasonCode),
            statusCode: statusCode);
    }

    private static async Task<IResult> ResendInvitationAsync(
        string organizationId,
        string invitationId,
        ClaimsPrincipal principal,
        OrganizationInvitationResendService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationInvitationReasonCodes.OrganizationNotFound);
        }

        if (!Guid.TryParse(invitationId, out var parsedInvitationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationInvitationReasonCodes.InvitationNotFound);
        }

        try
        {
            var result = await service.ResendAsync(
                actorUserId,
                parsedOrganizationId,
                parsedInvitationId,
                cancellationToken);
            if (result is OrganizationInvitationResendResult.Success success)
            {
                return TypedResults.Ok(OrganizationInvitationResendResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationInvitationResendStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationInvitationReasonCodes.OrganizationNotFound),
                OrganizationInvitationResendStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationInvitationReasonCodes.InsufficientPermission),
                OrganizationInvitationResendStatus.InvitationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationInvitationReasonCodes.InvitationNotFound),
                OrganizationInvitationResendStatus.Superseded =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.ResendSuperseded),
                OrganizationInvitationResendStatus.AlreadyMember =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.AlreadyMember),
                OrganizationInvitationResendStatus.InvitationAlreadyPending =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.InvitationAlreadyPending),
                OrganizationInvitationResendStatus.NoActiveSeatCapacity =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.NoActiveSeatCapacity),
                OrganizationInvitationResendStatus.SeatCapacityReached =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.SeatCapacityReached),
                OrganizationInvitationResendStatus.Resent =>
                    throw new InvalidOperationException(
                        "A successful resend result did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported invitation resend status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> AcceptInvitationAsync(
        ClaimsPrincipal principal,
        OrganizationInvitationAcceptanceRequest request,
        OrganizationInvitationAcceptanceService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }


        try
        {
            var result = await service.AcceptAsync(
                actorUserId,
                request.Secret!,
                cancellationToken);
            if (result is OrganizationInvitationAcceptanceResult.Success success)
            {
                return TypedResults.Ok(
                    OrganizationInvitationAcceptanceResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationInvitationAcceptanceStatus.InvitationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationInvitationReasonCodes.InvitationNotFound),
                OrganizationInvitationAcceptanceStatus.EmailMismatch =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationInvitationReasonCodes.EmailMismatch),
                OrganizationInvitationAcceptanceStatus.AlreadyMember =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.AlreadyMember),
                OrganizationInvitationAcceptanceStatus.Accepted =>
                    throw new InvalidOperationException(
                        "A successful acceptance result did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported invitation acceptance status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> ListMembersAsync(
        string organizationId,
        ClaimsPrincipal principal,
        OrganizationMemberListingService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
        }

        try
        {
            var result = await service.ListAsync(
                actorUserId,
                parsedOrganizationId,
                cancellationToken);
            if (result is OrganizationMemberListingResult.Success success)
            {
                return TypedResults.Ok(OrganizationMemberListResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationMemberListingStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.OrganizationNotFound),
                OrganizationMemberListingStatus.Listed =>
                    throw new InvalidOperationException(
                        "A successful member listing result did not contain its payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported organization member listing status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> ListInvitationsAsync(
        string organizationId,
        ClaimsPrincipal principal,
        OrganizationInvitationListingService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationInvitationReasonCodes.OrganizationNotFound);
        }

        try
        {
            var result = await service.ListAsync(
                actorUserId,
                parsedOrganizationId,
                cancellationToken);
            if (result is OrganizationInvitationListingResult.Success success)
            {
                return TypedResults.Ok(OrganizationInvitationListResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationInvitationListingStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationInvitationReasonCodes.OrganizationNotFound),
                OrganizationInvitationListingStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationInvitationReasonCodes.InsufficientPermission),
                OrganizationInvitationListingStatus.Listed =>
                    throw new InvalidOperationException(
                        "A successful invitation listing result did not contain its payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported organization invitation listing status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> RemoveMemberAsync(
        string organizationId,
        string membershipId,
        ClaimsPrincipal principal,
        OrganizationMemberRemovalService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
        }

        try
        {
            var result = await service.RemoveAsync(
                actorUserId,
                parsedOrganizationId,
                membershipId,
                cancellationToken);
            if (result is OrganizationMemberRemovalResult.Success success)
            {
                return TypedResults.Ok(OrganizationMemberRemovalResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationMemberRemovalStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.OrganizationNotFound),
                OrganizationMemberRemovalStatus.MemberNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.MemberNotFound),
                OrganizationMemberRemovalStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationReasonCodes.InsufficientPermission),
                OrganizationMemberRemovalStatus.OwnershipTransferRequired =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.OwnershipTransferRequired),
                OrganizationMemberRemovalStatus.ConcurrentModification =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.MembersChanged),
                OrganizationMemberRemovalStatus.Removed =>
                    throw new InvalidOperationException(
                        "A successful member removal did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported organization member removal status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> ChangeMemberRoleAsync(
        string organizationId,
        string membershipId,
        ClaimsPrincipal principal,
        OrganizationMemberRoleChangeRequest request,
        OrganizationRoleManagementService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
        }

        _ = OrganizationRoleNames.TryParseNonOwner(request.Role, out var role);

        try
        {
            var result = await service.ChangeRoleAsync(
                actorUserId,
                parsedOrganizationId,
                membershipId,
                role,
                cancellationToken);
            if (result is OrganizationRoleManagementResult.RoleChanged success)
            {
                return TypedResults.Ok(OrganizationMemberRoleChangeResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationRoleManagementStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.OrganizationNotFound),
                OrganizationRoleManagementStatus.MemberNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.MemberNotFound),
                OrganizationRoleManagementStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationReasonCodes.InsufficientPermission),
                OrganizationRoleManagementStatus.OwnershipTransferRequired =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.OwnershipTransferRequired),
                OrganizationRoleManagementStatus.RoleUnchanged =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.MemberRoleUnchanged),
                OrganizationRoleManagementStatus.ConcurrentModification =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.MembersChanged),
                OrganizationRoleManagementStatus.RoleChanged =>
                    throw new InvalidOperationException(
                        "A successful role change did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported member role-change status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> ChangeMemberSeatAsync(
        string organizationId,
        string membershipId,
        ClaimsPrincipal principal,
        OrganizationSeatAssignmentRequest request,
        OrganizationSeatAssignmentService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
        }


        try
        {
            var result = await service.SetAssignedAsync(
                actorUserId,
                parsedOrganizationId,
                membershipId,
                request.Assigned!.Value,
                cancellationToken);
            if (result is OrganizationSeatAssignmentResult.Success success)
            {
                return TypedResults.Ok(OrganizationSeatAssignmentResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationSeatAssignmentStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.OrganizationNotFound),
                OrganizationSeatAssignmentStatus.MemberNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.MemberNotFound),
                OrganizationSeatAssignmentStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationReasonCodes.InsufficientPermission),
                OrganizationSeatAssignmentStatus.Unchanged =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.MemberSeatUnchanged),
                OrganizationSeatAssignmentStatus.NoActiveSeatCapacity =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.NoActiveSeatCapacity),
                OrganizationSeatAssignmentStatus.SeatCapacityReached =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationInvitationReasonCodes.SeatCapacityReached),
                OrganizationSeatAssignmentStatus.ConcurrentModification =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.MembersChanged),
                OrganizationSeatAssignmentStatus.Assigned
                    or OrganizationSeatAssignmentStatus.Unassigned =>
                    throw new InvalidOperationException(
                        "A successful seat assignment did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported seat-assignment status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> TransferOwnershipAsync(
        string organizationId,
        string membershipId,
        ClaimsPrincipal principal,
        OrganizationRoleManagementService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(organizationId, out var parsedOrganizationId))
        {
            return OrganizationError(
                StatusCodes.Status404NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
        }

        try
        {
            var result = await service.TransferOwnershipAsync(
                actorUserId,
                parsedOrganizationId,
                membershipId,
                cancellationToken);
            if (result is OrganizationRoleManagementResult.OwnershipTransferred success)
            {
                return TypedResults.Ok(OrganizationOwnershipTransferResponse.From(success));
            }

            return result.Status switch
            {
                OrganizationRoleManagementStatus.OrganizationNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.OrganizationNotFound),
                OrganizationRoleManagementStatus.MemberNotFound =>
                    OrganizationError(
                        StatusCodes.Status404NotFound,
                        OrganizationReasonCodes.MemberNotFound),
                OrganizationRoleManagementStatus.InsufficientPermission =>
                    OrganizationError(
                        StatusCodes.Status403Forbidden,
                        OrganizationReasonCodes.InsufficientPermission),
                OrganizationRoleManagementStatus.InvalidOwnershipTarget =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.InvalidOwnershipTarget),
                OrganizationRoleManagementStatus.ConcurrentModification =>
                    OrganizationError(
                        StatusCodes.Status409Conflict,
                        OrganizationReasonCodes.MembersChanged),
                OrganizationRoleManagementStatus.OwnershipTransferred =>
                    throw new InvalidOperationException(
                        "A successful ownership transfer did not contain its required payload."),
                _ => throw new InvalidOperationException(
                    $"Unsupported ownership-transfer status: {result.Status}."),
            };
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

}
