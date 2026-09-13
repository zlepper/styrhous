using System.ComponentModel.DataAnnotations;

namespace Styrhous.Licensing.Api.Validation;

[AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
internal sealed class NormalizedAllowedValuesAttribute(params string[] values)
    : ValidationAttribute("The value is not supported.")
{
    public override bool IsValid(object? value)
    {
        return value is null || value is string text
            && values.Contains(text.Trim(), StringComparer.OrdinalIgnoreCase);
    }
}
