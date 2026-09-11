namespace Styrhous.Licensing.Runtime;

public enum LicensingRuntimeMode
{
    Api,
    Worker,
    Maintenance,
    MaintenanceLambda,
    Migrate,
}

public sealed record LicensingCommand(
    LicensingRuntimeMode Mode,
    IReadOnlyList<string> HostArguments)
{
    public static LicensingCommand Parse(IReadOnlyList<string> arguments)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        if (arguments.Count == 0
            || arguments[0].Length > 0 && arguments[0][0] == '-')
        {
            return new LicensingCommand(LicensingRuntimeMode.Api, [.. arguments]);
        }

        var mode = arguments[0].ToLowerInvariant() switch
        {
            "api" => LicensingRuntimeMode.Api,
            "worker" => LicensingRuntimeMode.Worker,
            "maintenance" => LicensingRuntimeMode.Maintenance,
            "maintenance-lambda" => LicensingRuntimeMode.MaintenanceLambda,
            "migrate" => LicensingRuntimeMode.Migrate,
            _ => throw new ArgumentException(
                "The first argument must be api, worker, maintenance, maintenance-lambda, "
                    + "or migrate.",
                nameof(arguments)),
        };
        return new LicensingCommand(mode, arguments.Skip(1).ToArray());
    }
}
