using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Api.Validation;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationCreationRequest(
    [property: Required, ValidEmailAddress] string? Email,
    [property: Required, NonOwnerOrganizationRole] string? Role,
    bool? AssignProductSeat);
