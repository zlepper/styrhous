using System.Reflection;
using System.Reflection.Emit;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
public sealed class EfPersistenceBoundaryTests
{
    [Test]
    public void ProductionAssemblyDoesNotReferenceRawSqlApis()
    {
        using var stream = File.OpenRead(typeof(LicensingDbContext).Assembly.Location);
        using var peReader = new PEReader(stream);
        var metadata = peReader.GetMetadataReader();
        var referencedMethods = metadata.MemberReferences
            .Select(handle => metadata.GetMemberReference(handle))
            .Select(reference => metadata.GetString(reference.Name))
            .ToHashSet(StringComparer.Ordinal);

        Assert.That(referencedMethods, Does.Contain("ExecuteUpdateAsync"),
            "The metadata scan must observe known EF persistence calls.");
        Assert.That(referencedMethods.Where(IsEfRawSqlApi), Is.Empty,
            "Application persistence must stay behind EF query and mutation abstractions.");
        // Rebus requires the existing provider connection and transaction. Only this
        // adapter may bridge EF to the native outbox; it still cannot execute raw SQL.
        var violations = typeof(LicensingDbContext).Assembly.GetTypes()
            .SelectMany(type => type.GetMethods(BindingFlags.Public | BindingFlags.NonPublic
                    | BindingFlags.Static | BindingFlags.Instance | BindingFlags.DeclaredOnly)
                .Cast<MethodBase>().Concat(type.GetConstructors(BindingFlags.Public
                    | BindingFlags.NonPublic | BindingFlags.Static | BindingFlags.Instance)))
            .SelectMany(method => ReadReferencedMembers(method)
                .Where(member => (IsRawSqlApi(member.Name)
                        || IsDirectDatabaseAccessType(member is Type type ? type.Name : member.DeclaringType?.Name ?? ""))
                    && !IsNativeOutboxConnectionAccess(method, member))
                .Select(member => $"{method.DeclaringType?.FullName}.{method.Name}: {member.Name}"));
        Assert.That(violations, Is.Empty,
            "Only the native Rebus adapter may access the underlying EF connection.");
    }

    private static bool IsNativeOutboxConnectionAccess(MethodBase method, MemberInfo member)
    {
        return method.DeclaringType is not null && IsNativeOutboxAdapter(method.DeclaringType)
            && (member is Type && member.Name is "NpgsqlConnection" or "DbConnection"
                || member.Name is "GetDbConnection" or "GetDbTransaction");
    }

    private static bool IsNativeOutboxAdapter(Type type)
    {
        return type == typeof(PostgresBackgroundWorkOutbox)
            || type.DeclaringType is not null && IsNativeOutboxAdapter(type.DeclaringType);
    }

    private static readonly Dictionary<short, OpCode> OpCodesByValue = typeof(OpCodes)
        .GetFields(BindingFlags.Public | BindingFlags.Static)
        .Select(field => (OpCode)field.GetValue(null)!)
        .ToDictionary(opCode => opCode.Value);

    private static IEnumerable<MemberInfo> ReadReferencedMembers(MethodBase method)
    {
        var il = method.GetMethodBody()?.GetILAsByteArray();
        if (il is null)
        {
            yield break;
        }
        for (var offset = 0; offset < il.Length;)
        {
            var first = il[offset++];
            var value = first == 0xfe ? unchecked((short)(0xfe00 | il[offset++])) : (short)first;
            var opCode = OpCodesByValue[value];
            if (opCode.OperandType is OperandType.InlineMethod or OperandType.InlineType
                or OperandType.InlineField or OperandType.InlineTok)
            {
                var member = method.Module.ResolveMember(BitConverter.ToInt32(il, offset),
                    method.DeclaringType?.GetGenericArguments(),
                    method.IsGenericMethod ? method.GetGenericArguments() : null);
                if (member is not null)
                {
                    yield return member;
                }
            }
            offset += opCode.OperandType switch
            {
                OperandType.InlineNone => 0,
                OperandType.ShortInlineBrTarget or OperandType.ShortInlineI or OperandType.ShortInlineVar => 1,
                OperandType.InlineVar => 2,
                OperandType.InlineI8 or OperandType.InlineR => 8,
                OperandType.InlineSwitch => 4 + 4 * BitConverter.ToInt32(il, offset),
                _ => 4,
            };
        }
    }

    [Test]
    public void TestAssemblyDoesNotReferenceEfRawSqlApis()
    {
        var referencedMethods = ReadReferencedMethodNames(typeof(EfPersistenceBoundaryTests).Assembly);

        Assert.Multiple(() =>
        {
            Assert.That(
                referencedMethods,
                Does.Contain("ExecuteUpdateAsync"),
                "The metadata scan must observe known EF test-fixture mutations.");
            Assert.That(
                referencedMethods.Where(IsEfRawSqlApi),
                Is.Empty,
                "Test fixtures must use EF query and mutation abstractions too.");
        });
    }

    private static HashSet<string> ReadReferencedMethodNames(System.Reflection.Assembly assembly)
    {
        using var stream = File.OpenRead(assembly.Location);
        using var peReader = new PEReader(stream);
        var metadata = peReader.GetMetadataReader();
        return metadata.MemberReferences
            .Select(handle => metadata.GetMemberReference(handle))
            .Select(reference => metadata.GetString(reference.Name))
            .ToHashSet(StringComparer.Ordinal);
    }

    private static bool IsRawSqlApi(string methodName)
    {
        return IsEfRawSqlApi(methodName)
        || methodName is "CreateCommand" or "CreateDbCommand" or "GetDbConnection" or "GetDbTransaction"
        || methodName == "set_CommandText"
        || methodName.StartsWith("ExecuteReader", StringComparison.Ordinal)
        || methodName.StartsWith("ExecuteNonQuery", StringComparison.Ordinal)
        || methodName.StartsWith("ExecuteScalar", StringComparison.Ordinal);
    }

    private static bool IsEfRawSqlApi(string methodName)
    {
        return methodName is "FromSql" or "FromSqlRaw" or "FromSqlInterpolated"
        || methodName is "ExecuteSql" or "ExecuteSqlAsync"
        || methodName.StartsWith("ExecuteSqlRaw", StringComparison.Ordinal)
        || methodName.StartsWith("ExecuteSqlInterpolated", StringComparison.Ordinal)
        || methodName == "SqlQuery"
        || methodName.StartsWith("SqlQueryRaw", StringComparison.Ordinal)
        || methodName.StartsWith("SqlQueryInterpolated", StringComparison.Ordinal)
        || methodName == "ToSqlQuery";
    }

    private static bool IsDirectDatabaseAccessType(string typeName)
    {
        return typeName is "DbCommand" or "DbConnection" or "DbDataReader"
        or "DbDataSource" or "DbParameter" or "DbParameterCollection"
        or "NpgsqlCommand" or "NpgsqlConnection" or "NpgsqlDataReader"
        or "NpgsqlDataSource" or "NpgsqlParameter";
    }
}
