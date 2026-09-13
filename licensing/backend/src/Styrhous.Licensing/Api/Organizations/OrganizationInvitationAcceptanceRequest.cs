using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Application.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationInvitationAcceptanceRequest(
    [property: Required, StringLength(OrganizationInvitationSecret.MaximumLength)] string? Secret);
