using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationCreationRequest(string? Name)
{
    [Required(ErrorMessage = "A valid organization name is required.")]
    [StringLength(Organization.MaximumNameLength, ErrorMessage = "A valid organization name is required.")]
    public string? Name { get; init; } = Name?.Trim();
}
