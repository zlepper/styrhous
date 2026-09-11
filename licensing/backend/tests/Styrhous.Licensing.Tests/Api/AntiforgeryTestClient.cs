using System.Text.Json;
using Styrhous.Licensing.Api.Antiforgery;

namespace Styrhous.Licensing.Tests.Api;

internal static class AntiforgeryTestClient
{
    public static async Task<string> GetTokenAsync(HttpClient client)
    {
        using var response = await client.GetAsync("/api/antiforgery");
        response.EnsureSuccessStatusCode();
        using var body = await JsonDocument.ParseAsync(
            await response.Content.ReadAsStreamAsync());
        return body.RootElement.GetProperty("requestToken").GetString()!;
    }

    public static void AddToken(HttpRequestMessage request, string token)
    {
        request.Headers.Add(AntiforgeryEndpoints.HeaderName, token);
    }
}
