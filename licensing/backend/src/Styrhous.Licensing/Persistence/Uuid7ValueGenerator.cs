using Microsoft.EntityFrameworkCore.ChangeTracking;
using Microsoft.EntityFrameworkCore.ValueGeneration;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Persistence;

public sealed class Uuid7ValueGenerator : ValueGenerator<Guid>
{
    public override bool GeneratesTemporaryValues => false;

    public override Guid Next(EntityEntry entry)
    {
        return Uuid7.Create();
    }
}
