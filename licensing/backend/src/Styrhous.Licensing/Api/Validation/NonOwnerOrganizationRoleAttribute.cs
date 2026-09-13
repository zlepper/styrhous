using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Api.Organizations;

namespace Styrhous.Licensing.Api.Validation;

[AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
internal sealed class NonOwnerOrganizationRoleAttribute()
    : ValidationAttribute("The role must be admin or member.")
{
    public override bool IsValid(object? value)
    {
        return value is null || value is string text && OrganizationRoleNames.TryParseNonOwner(text, out _);
    }
}
