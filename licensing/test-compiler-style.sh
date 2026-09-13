#!/usr/bin/env bash
set -euo pipefail

licensing_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixture_root=$(mktemp -d)
trap 'rm -rf "$fixture_root"' EXIT
cp "$licensing_root/CompilerStyle.props" "$fixture_root/CompilerStyle.props"

for area in backend infrastructure; do
  fixture="$fixture_root/$area"
  mkdir -p "$fixture"
  cp "$licensing_root/$area/Directory.Build.props" "$fixture/Directory.Build.props"
  cp "$licensing_root/.editorconfig" "$fixture/.editorconfig"
  cat > "$fixture/Style.csproj" <<'PROJECT'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>
</Project>
PROJECT
  cat > "$fixture/Style.cs" <<'CSHARP'
namespace Style;

public static class Example
{
    public static int Value() => 1;
    public static int Local()
    {
        static int Read() => 2;
        return Read();
    }
}
CSHARP
  if dotnet build "$fixture/Style.csproj" --nologo > "$fixture/build.log" 2>&1; then
    echo "$area build accepted expression-bodied methods." >&2
    exit 1
  fi
  for diagnostic in IDE0022 IDE0061; do
    if ! grep -Fq "error $diagnostic:" "$fixture/build.log"; then
      cat "$fixture/build.log" >&2
      echo "$area build did not enforce $diagnostic." >&2
      exit 1
    fi
  done
  cat > "$fixture/Style.cs" <<'CSHARP'
namespace Style;

public static class Example
{
    public static int Value()
    {
        return 1;
    }

    public static int Local()
    {
        static int Read()
        {
            return 2;
        }

        return Read();
    }
}
CSHARP
  dotnet build "$fixture/Style.csproj" --nologo --no-restore > "$fixture/build.log" 2>&1 || {
    cat "$fixture/build.log" >&2
    exit 1
  }
done
echo 'Compiler style rules reject expression bodies and accept block bodies.'
