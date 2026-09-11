namespace Styrhous.Licensing.Api.Desktop;

public static class DesktopProtocolMetadataEndpoints
{
    private const string OpenApiResourceName =
        "Styrhous.Licensing.Api.Desktop.desktop-v1-openapi.json";

    private static readonly byte[] OpenApiDocument = LoadOpenApiDocument();

    public static IEndpointRouteBuilder MapDesktopProtocolMetadataEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        ArgumentNullException.ThrowIfNull(endpoints);

        endpoints.MapGet(
            "/desktop/v1/openapi.json",
            () => Results.Bytes(OpenApiDocument, "application/json; charset=utf-8"));
        endpoints.MapGet(
            "/desktop/v1/device/verify",
            (string? user_code) => Results.Redirect(
                string.IsNullOrWhiteSpace(user_code)
                    ? "/devices/authorize"
                    : $"/devices/authorize?user_code={Uri.EscapeDataString(user_code)}"));
        return endpoints;
    }

    private static byte[] LoadOpenApiDocument()
    {
        using var resource = typeof(DesktopProtocolMetadataEndpoints)
            .Assembly
            .GetManifestResourceStream(OpenApiResourceName)
            ?? throw new InvalidOperationException(
                $"Embedded desktop protocol document {OpenApiResourceName} is missing.");
        using var buffer = new MemoryStream();
        resource.CopyTo(buffer);
        return buffer.ToArray();
    }
}
