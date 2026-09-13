namespace Styrhous.Licensing.Domain.Validation;

internal static class RequiredText
{
    public static string Normalize(
        string value,
        string parameterName,
        int maximumLength,
        string description = "value")
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(value, parameterName);
        var normalized = value.Trim();
        if (normalized.Length > maximumLength)
        {
            throw new ArgumentException(
                $"The {description} must be no longer than {maximumLength} characters.",
                parameterName);
        }

        return normalized;
    }
}
