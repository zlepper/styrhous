using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Api.Billing;
using Styrhous.Licensing.Api.Organizations;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
public sealed class RequestModelValidationTests
{
    [TestCase(" ADMIN ", true)]
    [TestCase("Member", true)]
    [TestCase("admİn", false)]
    [TestCase("owner", false)]
    [SetCulture("tr-TR")]
    public void OrganizationRoleValidationUsesInvariantApiNames(string role, bool valid)
    {
        var invitation = new OrganizationInvitationCreationRequest("person@example.com", role, true);
        var change = new OrganizationMemberRoleChangeRequest(role);
        Assert.Multiple(() =>
        {
            Assert.That(IsValid(invitation), Is.EqualTo(valid));
            Assert.That(IsValid(change), Is.EqualTo(valid));
        });
    }

    [TestCase(" MONTHLY ", true)]
    [TestCase("Annual", true)]
    [TestCase("weekly", false)]
    [TestCase("", false)]
    [SetCulture("tr-TR")]
    public void BillingCadenceValidationUsesInvariantApiNames(string cadence, bool valid)
    {
        Assert.That(IsValid(new BillingCheckoutRequest(cadence, 1, null)), Is.EqualTo(valid));
    }

    private static bool IsValid(object request)
    {
        return Validator.TryValidateObject(request, new ValidationContext(request), [], validateAllProperties: true);
    }
}
