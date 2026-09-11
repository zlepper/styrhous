using System.Data.Common;
using Microsoft.EntityFrameworkCore.Diagnostics;

namespace Styrhous.Licensing.Tests.Persistence;

internal enum SignupBarrierQuery
{
    EmailAvailability,
    EmailClaimOwner,
}

internal sealed class SignupPreflightBarrierInterceptor(
    DatabaseCommandBarrier barrier,
    SignupBarrierQuery query = SignupBarrierQuery.EmailAvailability)
    : DbCommandInterceptor
{
    private bool _hasWaited;

    public override async ValueTask<DbDataReader> ReaderExecutedAsync(
        DbCommand command,
        CommandExecutedEventData eventData,
        DbDataReader result,
        CancellationToken cancellationToken = default)
    {
        if (!_hasWaited && MatchesQuery(command.CommandText))
        {
            _hasWaited = true;
            await barrier.SignalAndWaitAsync(cancellationToken);
        }

        return result;
    }

    private bool MatchesQuery(string commandText)
    {
        return commandText.Contains("normalized_email", StringComparison.Ordinal)
        && query switch
        {
            SignupBarrierQuery.EmailAvailability =>
                commandText.Contains("EXISTS", StringComparison.OrdinalIgnoreCase),
            SignupBarrierQuery.EmailClaimOwner =>
                commandText.Contains("FROM verified_email_claims", StringComparison.Ordinal),
            _ => false,
        };
    }
}
