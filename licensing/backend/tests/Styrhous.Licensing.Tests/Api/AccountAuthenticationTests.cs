using System.Text.Json;
using System.Security.Claims;
using Microsoft.IdentityModel.JsonWebTokens;
using Styrhous.Licensing.Api.Authentication;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class AccountAuthenticationTests
{
    [Test]
    public void GitHubSelectsThePrimaryVerifiedEmail()
    {
        using var response = JsonDocument.Parse(
            """
            [
              { "email": "unverified@example.com", "verified": false, "primary": true },
              { "email": "secondary@example.com", "verified": true, "primary": false },
              { "email": "primary@example.com", "verified": true, "primary": true }
            ]
            """);

        Assert.That(
            AccountAuthentication.GetGitHubVerifiedEmail(response.RootElement),
            Is.EqualTo("primary@example.com"));
    }

    [TestCase("[]")]
    [TestCase("{}")]
    [TestCase("[{ \"email\": \"person@example.com\", \"verified\": false, \"primary\": true }]")]
    [TestCase("[{ \"verified\": true, \"primary\": true }]")]
    public void GitHubRejectsMissingOrUnverifiedEmail(string json)
    {
        using var response = JsonDocument.Parse(json);

        Assert.That(
            AccountAuthentication.GetGitHubVerifiedEmail(response.RootElement),
            Is.Null);
    }

    [TestCase(true, "person@example.com", "person@example.com")]
    [TestCase(false, "person@example.com", null)]
    [TestCase(true, null, null)]
    public void GoogleRequiresTheProviderVerifiedEmailFlag(
        bool verified,
        string? email,
        string? expected)
    {
        using var response = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            email_verified = verified,
            email,
        }));

        Assert.That(
            AccountAuthentication.GetGoogleVerifiedEmail(response.RootElement),
            Is.EqualTo(expected));
    }

    [TestCase("{}", null)]
    [TestCase("{\"verified_primary_email\":true}", null)]
    [TestCase("{\"verified_secondary_email\":123}", null)]
    [TestCase("{\"verified_primary_email\":{\"email\":\"wrong@example.com\"}}", null)]
    [TestCase("{\"mail\":\"wrong@example.com\",\"userPrincipalName\":\"wrong@example.com\"}", null)]
    [TestCase("{\"email\":\"wrong@example.com\"}", null)]
    [TestCase("{\"email\":\"wrong@example.com\",\"xms_edov\":false}", null)]
    [TestCase("{\"email\":\"wrong@example.com\",\"xms_edov\":\"true\"}", null)]
    [TestCase("{\"xms_edov\":true}", null)]
    [TestCase("{\"email\":\"right@example.com\",\"xms_edov\":true}", "right@example.com")]
    [TestCase("{\"verified_primary_email\":\"primary@example.com\",\"email\":\"wrong@example.com\"}", "primary@example.com")]
    [TestCase("{\"verified_secondary_email\":[\"secondary@example.com\",\"other@example.com\"]}", "secondary@example.com")]
    [TestCase("{\"verified_primary_email\":\" \",\"verified_secondary_email\":\"secondary@example.com\"}", "secondary@example.com")]
    public void MicrosoftOnlyAcceptsAuthoritativeEmailClaims(string payload, string? expected)
    {
        var token = new JsonWebToken("{\"alg\":\"none\"}", payload);
        Assert.That(
            MicrosoftAccountAuthentication.GetVerifiedEmail(new ClaimsPrincipal(new ClaimsIdentity(token.Claims))),
            Is.EqualTo(expected));
    }
}
