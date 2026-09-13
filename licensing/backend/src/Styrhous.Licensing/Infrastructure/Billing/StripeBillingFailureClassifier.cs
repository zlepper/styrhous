using System.Net;

namespace Styrhous.Licensing.Infrastructure.Billing;

internal static class StripeBillingFailureClassifier
{
    public static bool IsTransient(HttpStatusCode statusCode)
    {
        var numericStatus = (int)statusCode;
        return statusCode is HttpStatusCode.RequestTimeout or HttpStatusCode.Conflict
            || numericStatus == StatusCodes.Status429TooManyRequests
            || numericStatus >= StatusCodes.Status500InternalServerError;
    }

    public static bool IsCachedServerFailure(HttpStatusCode statusCode)
    {
        return statusCode == HttpStatusCode.InternalServerError;
    }
}
