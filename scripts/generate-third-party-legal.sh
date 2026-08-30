#!/usr/bin/env bash

set -euo pipefail

readonly expected_cargo_about_version="cargo-about 0.9.1"
script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_directory
repository_root="$(cd "$script_directory/.." && pwd)"
readonly repository_root
readonly legal_build_directory="$repository_root/target/legal"
readonly generated_html="$legal_build_directory/THIRD_PARTY_LICENSES.html"
readonly generated_json="$legal_build_directory/third-party-licenses.json"
readonly generated_source="$legal_build_directory/THIRD_PARTY_SOURCE.tar.gz"
readonly tracked_html="$repository_root/THIRD_PARTY_LICENSES.html"
readonly tracked_source="$repository_root/THIRD_PARTY_SOURCE.tar.gz"
readonly legal_resources="$repository_root/legal-resources.tsv"
readonly source_license_policy="$repository_root/legal-source-licenses.txt"
readonly release_targets="$repository_root/legal-release-targets.tsv"

usage() {
    echo "usage: $0 [--check|--update]" >&2
}

mode="${1:---check}"
case "$mode" in
    --check | --update) ;;
    *)
        usage
        exit 2
        ;;
esac

cd "$repository_root"

actual_cargo_about_version="$(cargo about --version)"
if [[ "$actual_cargo_about_version" != "$expected_cargo_about_version" ]]; then
    echo "expected $expected_cargo_about_version, found $actual_cargo_about_version" >&2
    exit 1
fi

command -v perl >/dev/null
command -v tar >/dev/null
command -v gzip >/dev/null

tar_version="$(tar --version)"
if [[ "$tar_version" != *"GNU tar"* ]]; then
    echo "legal source generation requires GNU tar" >&2
    exit 1
fi

# Keep the machine-readable legal resource manifest synchronized with the
# corresponding cargo-packager mappings. Other application resources are
# outside this manifest and remain free to use files or globs.
cargo metadata --no-deps --format-version=1 | perl -MJSON::PP -MCwd=abs_path -MFile::Basename=dirname -e '
    my ($resource_manifest, $repository_root) = @ARGV;
    my %expected;
    open my $manifest, q{<}, $resource_manifest or die "$resource_manifest: $!\n";
    while (my $line = <$manifest>) {
        chomp $line;
        next if $line eq q{} || $line =~ /^#/;
        my ($repository_path, $target, $release_asset) = split /\t/, $line, -1;
        die "invalid legal resource manifest line: $line\n"
            unless defined $release_asset && $release_asset =~ /^(?:yes|no)$/;
        my $source = abs_path("$repository_root/$repository_path")
            or die "missing legal resource: $repository_path\n";
        die "duplicate packaged legal filename: $target\n" if $expected{$target};
        $expected{$target} = $source;
    }

    local $/;
    my $data = decode_json(<STDIN>);
    my ($package) = grep { $_->{name} eq q{styrhous} } @{$data->{packages}};
    die "styrhous package metadata not found\n" unless $package;
    my $package_directory = dirname($package->{manifest_path});
    my %actual;
    for my $resource (@{$package->{metadata}{packager}{resources}}) {
        my $target = $resource->{target};
        next unless $expected{$target};
        my $source = abs_path("$package_directory/$resource->{src}")
            or die "missing cargo-packager legal resource: $resource->{src}\n";
        die "duplicate cargo-packager legal target: $target\n" if $actual{$target};
        $actual{$target} = $source;
    }

    for my $target (sort keys %expected) {
        die "cargo-packager is missing legal target $target\n" unless $actual{$target};
        die "cargo-packager maps $target from the wrong source\n"
            unless $actual{$target} eq $expected{$target};
    }
' "$legal_resources" "$repository_root"

# cargo-about must cover every native release target. The Debian release jobs
# reuse their corresponding Linux target and therefore carry a -deb suffix.
perl -e '
    my ($target_manifest, $about_path, $ci_path, $release_path) = @ARGV;
    my (%expected_ids, %expected_triples);
    open my $manifest, q{<}, $target_manifest or die "$target_manifest: $!\n";
    while (my $line = <$manifest>) {
        chomp $line;
        next if $line eq q{} || $line =~ /^#/;
        my ($id, $triple) = split /\t/, $line, -1;
        die "invalid release target manifest line: $line\n"
            unless defined $triple && $id ne q{} && $triple ne q{};
        die "duplicate release target id: $id\n" if $expected_ids{$id};
        die "duplicate cargo target triple: $triple\n" if $expected_triples{$triple};
        $expected_ids{$id} = 1;
        $expected_triples{$triple} = 1;
    }

    open my $about, q{<}, $about_path or die "$about_path: $!\n";
    local $/;
    my $about_text = <$about>;
    my ($target_block) = $about_text =~ /targets\s*=\s*\[(.*?)\]/s;
    die "about.toml targets list not found\n" unless defined $target_block;
    my %about_targets = map { $_ => 1 } ($target_block =~ /"([^"]+)"/g);
    for my $triple (keys %expected_triples) {
        die "about.toml is missing release target $triple\n" unless $about_targets{$triple};
    }
    for my $triple (keys %about_targets) {
        die "about.toml has unmanifested target $triple\n" unless $expected_triples{$triple};
    }

    for my $workflow_path ($ci_path, $release_path) {
        open my $workflow, q{<}, $workflow_path or die "$workflow_path: $!\n";
        local $/;
        my $workflow_text = <$workflow>;
        my @ids = ($workflow_text =~ /^\s+- id:\s*([^\s]+)\s*$/mg);
        my %seen;
        for my $id (@ids) {
            $id =~ s/-deb$// if $workflow_path eq $release_path;
            die "$workflow_path has unmanifested release target $id\n"
                unless $expected_ids{$id};
            $seen{$id} = 1;
        }
        for my $id (keys %expected_ids) {
            die "$workflow_path is missing release target $id\n" unless $seen{$id};
        }
    }
' "$release_targets" about.toml .github/workflows/ci.yml .github/workflows/release.yml

while IFS= read -r source_license; do
    if [[ -z "$source_license" ]] || [[ "$source_license" == \#* ]]; then
        continue
    fi
    if ! grep -Fq -- "\"$source_license\"" about.toml; then
        echo "$source_license must also be accepted in about.toml" >&2
        exit 1
    fi
done <"$source_license_policy"

# Fetch the complete locked graph once, then force cargo-about to use only the
# local immutable crate contents while resolving licenses.
cargo fetch --locked
mkdir -p "$legal_build_directory"

cargo about generate \
    --workspace \
    --locked \
    --offline \
    --fail \
    --format json \
    --output-file "$generated_json"

cargo about generate \
    --workspace \
    --locked \
    --offline \
    --fail \
    about.hbs \
    --output-file "$generated_html"

# License files published by crates sometimes contain CRLF endings or trailing
# spaces. Normalize those source artifacts so the generated report is stable
# across platforms and remains clean under Git's whitespace checks.
perl -pi -e 's/[ \t\r]+$//' "$generated_html"

# Build a deterministic corresponding-source archive for every dependency for
# which cargo-about selected a license listed in legal-source-licenses.txt.
readonly source_list="$legal_build_directory/source-available-dependencies.tsv"
perl -MJSON::PP -e '
    my ($policy_path, $json_path) = @ARGV;
    my %source_licenses;
    open my $policy, q{<}, $policy_path or die "$policy_path: $!\n";
    while (my $line = <$policy>) {
        chomp $line;
        next if $line eq q{} || $line =~ /^#/;
        $source_licenses{$line} = 1;
    }
    local $/;
    open my $json, q{<}, $json_path or die "$json_path: $!\n";
    my $data = decode_json(<$json>);
    my %seen;
    for my $license (@{$data->{licenses}}) {
        next unless $source_licenses{$license->{id} // q{}};
        for my $use (@{$license->{used_by}}) {
            my $crate = $use->{crate};
            my $key = join qq{\0}, $crate->{name}, $crate->{version};
            next if $seen{$key}++;
            print join(qq{\t}, $crate->{name}, $crate->{version}, $crate->{manifest_path}), qq{\n};
        }
    }
' "$source_license_policy" "$generated_json" >"$source_list"

readonly source_staging="$legal_build_directory/corresponding-source"
rm -rf -- "$source_staging"
mkdir -p "$source_staging"

while IFS=$'\t' read -r crate_name crate_version manifest_path; do
    if [[ ! "$crate_name" =~ ^[A-Za-z0-9_.-]+$ ]] ||
        [[ ! "$crate_version" =~ ^[A-Za-z0-9+_.-]+$ ]]; then
        echo "unsafe crate identity in corresponding-source list" >&2
        exit 1
    fi

    crate_directory="${manifest_path%/Cargo.toml}"
    if [[ "$crate_directory" == "$manifest_path" ]] || [[ ! -d "$crate_directory" ]]; then
        echo "could not locate source for $crate_name $crate_version" >&2
        exit 1
    fi

    destination="$source_staging/$crate_name-$crate_version"
    mkdir -p "$destination"
    cp -a -- "$crate_directory/." "$destination/"
done <"$source_list"

tar \
    --sort=name \
    --mtime='UTC 1970-01-01' \
    --owner=0 \
    --group=0 \
    --numeric-owner \
    -cf - \
    -C "$source_staging" . | gzip -n >"$generated_source"

if [[ "$mode" == "--update" ]]; then
    cp -- "$generated_html" "$tracked_html"
    cp -- "$generated_source" "$tracked_source"
    echo "updated THIRD_PARTY_LICENSES.html and THIRD_PARTY_SOURCE.tar.gz"
    exit 0
fi

if ! cmp --silent "$tracked_html" "$generated_html"; then
    echo "THIRD_PARTY_LICENSES.html is stale; run $0 --update" >&2
    diff -u "$tracked_html" "$generated_html" || true
    exit 1
fi

if ! cmp --silent "$tracked_source" "$generated_source"; then
    echo "THIRD_PARTY_SOURCE.tar.gz is stale; run $0 --update" >&2
    exit 1
fi

echo "third-party legal artifacts are current"
