namespace Styrhous.Licensing.Persistence;

// The schema belongs to Rebus. Mapping it lets migrations provision it with the
// schema-owner credentials while runtime roles only read and write existing tables.
internal sealed class RebusOutboxMessage
{
    public long Id { get; set; }
    public string? CorrelationId { get; set; }
    public string? MessageId { get; set; }
    public string? SourceQueue { get; set; }
    public string DestinationAddress { get; set; } = string.Empty;
    public string? Headers { get; set; }
    public byte[]? Body { get; set; }
    public bool Sent { get; set; }
}
