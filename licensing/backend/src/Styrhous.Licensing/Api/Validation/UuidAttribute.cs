using System.ComponentModel.DataAnnotations;

namespace Styrhous.Licensing.Api.Validation;

[AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
internal sealed class UuidAttribute() : ValidationAttribute("The identifier must be a UUID.")
{
    public override bool IsValid(object? value)
    {
        return value is null || value is string text && Guid.TryParse(text, out _);
    }
}
