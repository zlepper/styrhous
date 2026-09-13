using System.ComponentModel.DataAnnotations;

namespace Styrhous.Licensing.Api.Organizations;

public sealed record OrganizationSeatAssignmentRequest(
    [property: Required] bool? Assigned);
