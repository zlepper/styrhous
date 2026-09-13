using System.Collections.Concurrent;
using Rebus.Messages;
using Rebus.Transport;

namespace Styrhous.Licensing.Infrastructure.Messaging;

// Rebus.PostgreSql 9.1.1's Retrier returns successfully when cancellation interrupts
// a failed-send retry delay. Its forwarder then commits the batch's DELETE. Keep
// this guard until upstream propagates cancellation from PostgreSql/Retrier.cs:
// https://github.com/rebus-org/Rebus.PostgreSql/blob/9.1.1/Rebus.PostgreSql/PostgreSql/Retrier.cs
internal sealed class NativeOutboxSendGuardTransport(ITransport inner) : ITransport
{
    private const string PendingSendsKey = "styrhous-native-outbox-pending-sends";

    public string Address => inner.Address;

    public void CreateQueue(string address)
    {
        inner.CreateQueue(address);
    }

    public async Task Send(string destinationAddress, TransportMessage message, ITransactionContext context)
    {
        var pending = context.GetOrAdd(PendingSendsKey, () =>
        {
            var sends = new ConcurrentDictionary<(TransportMessage Message, string Destination), byte>();
            context.OnCommit(_ =>
            {
                if (!sends.IsEmpty)
                {
                    throw new InvalidOperationException(
                        "An outbox send did not complete; the forwarding transaction cannot commit.");
                }

                return Task.CompletedTask;
            });
            return sends;
        });
        var key = (message, destinationAddress);
        pending[key] = 0;
        await inner.Send(destinationAddress, message, context);
        pending.TryRemove(key, out _);
    }

    public Task<TransportMessage> Receive(ITransactionContext context, CancellationToken cancellationToken)
    {
        return inner.Receive(context, cancellationToken);
    }
}
