namespace Styrhous.Licensing.Api.Desktop;

internal static class DesktopProtocolRequestState
{
    private static readonly object StateKey = new();
    private static readonly object AuthorizationDecisionKey = new();

    public static void SetAuthorizationDecision(
        HttpContext httpContext,
        Guid tokenId,
        DesktopAuthorizationDecision decision)
    {
        ArgumentNullException.ThrowIfNull(httpContext);
        httpContext.Items[AuthorizationDecisionKey] =
            new DesktopAuthorizationDecisionOperation(tokenId, decision);
    }

    public static bool TryGetAuthorizationDecision(
        HttpContext httpContext,
        out DesktopAuthorizationDecisionOperation operation)
    {
        ArgumentNullException.ThrowIfNull(httpContext);
        if (httpContext.Items.TryGetValue(AuthorizationDecisionKey, out var value)
            && value is DesktopAuthorizationDecisionOperation storedOperation)
        {
            operation = storedOperation;
            return true;
        }

        operation = default;
        return false;
    }

    public static void SetTokenGrant(
        HttpContext httpContext,
        Guid userId,
        Guid authorizationId,
        Guid tokenId,
        DesktopTokenGrant grant)
    {
        ArgumentNullException.ThrowIfNull(httpContext);
        httpContext.Items[StateKey] = new DesktopProtocolOperation(
            userId,
            authorizationId,
            tokenId,
            grant);
    }

    public static void SetRevocation(
        HttpContext httpContext,
        Guid userId,
        Guid authorizationId)
    {
        ArgumentNullException.ThrowIfNull(httpContext);
        httpContext.Items[StateKey] = new DesktopProtocolOperation(
            userId,
            authorizationId,
            TokenId: null,
            TokenGrant: null);
    }

    public static bool TryGet(
        HttpContext httpContext,
        out DesktopProtocolOperation operation)
    {
        ArgumentNullException.ThrowIfNull(httpContext);
        if (httpContext.Items.TryGetValue(StateKey, out var value)
            && value is DesktopProtocolOperation storedOperation)
        {
            operation = storedOperation;
            return true;
        }

        operation = default;
        return false;
    }

    public static void Clear(HttpContext httpContext)
    {
        ArgumentNullException.ThrowIfNull(httpContext);
        httpContext.Items.Remove(StateKey);
        httpContext.Items.Remove(AuthorizationDecisionKey);
    }
}

internal enum DesktopTokenGrant
{
    DeviceCode,
    RefreshToken,
}

internal enum DesktopAuthorizationDecision
{
    Approve,
    Deny,
}

internal readonly record struct DesktopAuthorizationDecisionOperation(
    Guid TokenId,
    DesktopAuthorizationDecision Decision);

internal readonly record struct DesktopProtocolOperation(
    Guid UserId,
    Guid AuthorizationId,
    Guid? TokenId,
    DesktopTokenGrant? TokenGrant);
