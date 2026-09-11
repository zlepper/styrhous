using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Trials;

namespace Styrhous.Licensing.Domain.Signups;

public sealed class SignupRegistration
{
    private SignupRegistration(
        UserAccount user,
        ExternalIdentity externalIdentity,
        VerifiedEmailClaim verifiedEmailClaim,
        BillingAccount billingAccount,
        Seat seat,
        Trial trial)
    {
        User = user;
        ExternalIdentity = externalIdentity;
        VerifiedEmailClaim = verifiedEmailClaim;
        BillingAccount = billingAccount;
        Seat = seat;
        Trial = trial;
    }

    public UserAccount User { get; }

    public ExternalIdentity ExternalIdentity { get; }

    public VerifiedEmailClaim VerifiedEmailClaim { get; }

    public BillingAccount BillingAccount { get; }

    public Seat Seat { get; }

    public Trial Trial { get; }

    public static SignupRegistration Start(
        VerifiedExternalIdentity identity,
        DateTimeOffset observedAt)
    {
        ArgumentNullException.ThrowIfNull(identity);

        var utcObservedAt = observedAt.ToUniversalTime();
        var user = UserAccount.Create(identity, utcObservedAt);
        var externalIdentity = ExternalIdentity.Create(user.Id, identity, utcObservedAt);
        var verifiedEmailClaim = VerifiedEmailClaim.Create(user.Id, identity, utcObservedAt);
        var billingAccount = BillingAccount.CreatePersonal(user.Id, utcObservedAt);
        var seat = Seat.Assign(billingAccount.Id, user.Id, utcObservedAt);
        var trial = Trial.Start(user.Id, billingAccount.Id, utcObservedAt);
        return new SignupRegistration(
            user,
            externalIdentity,
            verifiedEmailClaim,
            billingAccount,
            seat,
            trial);
    }
}
