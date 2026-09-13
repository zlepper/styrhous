#!/usr/bin/env bash
set -euo pipefail

backend_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$backend_root"


dotnet restore Styrhous.Licensing.slnx --locked-mode
dotnet format Styrhous.Licensing.slnx --verify-no-changes --no-restore
dotnet build Styrhous.Licensing.slnx --configuration Release --no-restore --warnaserror
dotnet test Styrhous.Licensing.slnx --configuration Release --no-build --no-restore
