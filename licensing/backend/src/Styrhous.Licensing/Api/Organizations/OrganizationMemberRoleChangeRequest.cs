using Styrhous.Licensing.Api.Validation;
using System.ComponentModel.DataAnnotations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationMemberRoleChangeRequest(
    [property: Required, NonOwnerOrganizationRole] string? Role);
