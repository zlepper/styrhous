using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Api.Validation;

[AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
internal sealed class ValidEmailAddressAttribute()
    : ValidationAttribute("A valid email address is required.")
{
    public override bool IsValid(object? value)
    {
        return value is null || value is string text && EmailAddress.IsValid(text);
    }
}
