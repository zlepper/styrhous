# Theming Styrhous

Styrhous ships one built-in theme, **Harbour Dark**. You can change any of its colours with a
theme file.

## Where the theme file lives

Styrhous reads one optional file, `theme.toml`, from its data directory:

| Platform | Path |
| --- | --- |
| Linux | `$XDG_DATA_HOME/styrhous/theme.toml`, usually `~/.local/share/styrhous/theme.toml` |
| macOS | `~/Library/Application Support/Styrhous/theme.toml` |
| Windows | `%APPDATA%\Styrhous\data\theme.toml` |

Styrhous checks the file about once a second, so saving it updates the running application.
If the file is invalid, Styrhous keeps the current colours and shows the errors in a toast.
Delete the file to go back to Harbour Dark.

## Format

A theme file lists only the colours that differ from a built-in theme:

```toml
name    = "Harbour Dark · high contrast"
extends = "harbour-dark"   # built-in themes only

[palette]
sand-300 = "#B3A89B"       # lighter muted text

[roles]
text-muted  = "sand-200"
focus-ring  = "sky-300"
accent-fill = "coral-300"

[components]
pill-on-bg = "accent-tint"

[terminal]
ansi-blue = "sky-300"
```

- `name` (required) is the theme's display name.
- `extends` names the built-in theme the file starts from. Only `harbour-dark` exists, and it is
  the default. A theme file cannot extend another theme file.

The colours are defined in layers, and each layer refers only to the one below it (palette, then
roles and terminal, then components):

- **`[palette]`**: the raw colours. Assign a hex colour: `#RGB`, `#RGBA`, `#RRGGBB` or
  `#RRGGBBAA`. Changing a palette entry changes every role that uses it.
- **`[roles]`**: colours named by purpose, such as `surface-panel` or `text-muted`. Assign a
  palette entry name.
- **`[terminal]`**: the code, log, YAML syntax and ANSI colours. These are roles too, and you also
  assign them palette entry names. They have their own table so a terminal scheme can be changed
  on its own.
- **`[components]`**: optional overrides for single components, such as `row-bg` for table rows.
  Assign a role name.

Unknown keys, keys in the wrong table and names that do not exist are all reported together,
with a suggestion when a name looks like a typo.

## Key reference

Every key with its Harbour Dark value. This list is generated from the theme's tables; a test fails
when it is out of date.

<!-- BEGIN GENERATED KEY REFERENCE -->

### `palette`

| Key | Harbour Dark |
| --- | --- |
| `sand-50` | `#F5EFE6` |
| `sand-100` | `#E7DED3` |
| `sand-200` | `#BDB2A6` |
| `sand-300` | `#9C9184` |
| `sand-400` | `#8A7F74` |
| `sand-500` | `#6B6157` |
| `sand-600` | `#5A4F47` |
| `sand-700` | `#4A4039` |
| `sand-750` | `#3A322C` |
| `sand-800` | `#352E29` |
| `sand-825` | `#2F2925` |
| `sand-850` | `#2C2622` |
| `sand-875` | `#26211E` |
| `sand-900` | `#221D1A` |
| `sand-950` | `#1C1917` |
| `sand-975` | `#171412` |
| `coral-200` | `#FFB59C` |
| `coral-300` | `#FF9F80` |
| `coral-400` | `#FF8A65` |
| `coral-500` | `#F2764F` |
| `coral-700` | `#9A5541` |
| `coral-800` | `#553A30` |
| `coral-850` | `#45302A` |
| `coral-900` | `#3A2A23` |
| `red-100` | `#FFCDC8` |
| `red-300` | `#FF8F8F` |
| `red-350` | `#FF8282` |
| `red-400` | `#FF6B6B` |
| `red-500` | `#F25555` |
| `red-700` | `#8C3F3B` |
| `red-850` | `#472A27` |
| `red-900` | `#3A2422` |
| `amber-100` | `#FBE3A6` |
| `amber-400` | `#F2C14E` |
| `amber-700` | `#7A6128` |
| `amber-850` | `#4A3A1C` |
| `amber-900` | `#3A2F1A` |
| `mint-200` | `#A7F3D0` |
| `mint-400` | `#6EE7B7` |
| `mint-700` | `#2F6B53` |
| `mint-900` | `#1F322A` |
| `sky-100` | `#CDE8FD` |
| `sky-300` | `#A9D8FC` |
| `sky-400` | `#7CC4FA` |
| `sky-700` | `#335A78` |
| `sky-900` | `#1F2E3A` |
| `violet-400` | `#B39DFF` |
| `pink-200` | `#F8B4CB` |
| `pink-400` | `#F48FB1` |
| `teal-200` | `#7FE0D4` |
| `teal-400` | `#4DD0C1` |
| `lime-300` | `#C3E88D` |
| `green-300` | `#9ADBA0` |
| `black-a55` | `#0000008C` |
| `black-a45` | `#00000073` |
| `black-a32` | `#00000052` |
| `black-a18` | `#0000002E` |
| `white-a04` | `#FFFFFF0A` |
| `sand-950-a60` | `#120F0D99` |
| `sand-950-a25` | `#1C181840` |
| `sand-950-a18` | `#1C1C162E` |
| `coral-a55` | `#FF8A648C` |
| `coral-a16` | `#FF896429` |
| `red-a55` | `#FF6B6B8C` |
| `red-a14` | `#FF6A6A24` |
| `mint-a55` | `#6DE7B68C` |
| `amber-a55` | `#F2C14E8C` |
| `amber-a22` | `#F1BF4D38` |

### `roles`

| Key | Harbour Dark |
| --- | --- |
| `surface-canvas` | `sand-950` |
| `surface-sunken` | `sand-900` |
| `surface-panel` | `sand-875` |
| `surface-panel-hover` | `sand-850` |
| `surface-raised` | `sand-825` |
| `surface-raised-hover` | `sand-800` |
| `control-fill` | `sand-825` |
| `control-fill-hover` | `sand-750` |
| `control-fill-active` | `sand-700` |
| `control-fill-strong` | `sand-750` |
| `control-fill-strong-hover` | `sand-700` |
| `control-fill-strong-active` | `sand-600` |
| `control-fill-disabled` | `sand-900` |
| `control-border` | `sand-400` |
| `control-border-hover` | `sand-300` |
| `item-highlight` | `sand-750` |
| `item-selected` | `sand-750` |
| `divider` | `sand-750` |
| `divider-subtle` | `sand-825` |
| `divider-strong` | `sand-700` |
| `text-primary` | `sand-50` |
| `text-secondary` | `sand-200` |
| `text-muted` | `sand-300` |
| `text-disabled` | `sand-500` |
| `text-on-fill` | `sand-950` |
| `inverse-surface` | `sand-50` |
| `inverse-surface-hover` | `sand-100` |
| `inverse-text` | `sand-950` |
| `inverse-text-muted` | `sand-500` |
| `tint-on-fill` | `sand-950-a18` |
| `track-on-fill` | `sand-950-a25` |
| `accent-fill` | `coral-400` |
| `accent-fill-hover` | `coral-300` |
| `accent-fill-active` | `coral-500` |
| `accent-tint` | `coral-900` |
| `accent-tint-hover` | `coral-850` |
| `accent-tint-active` | `coral-800` |
| `accent-text` | `coral-400` |
| `accent-outline` | `coral-700` |
| `accent-glow` | `coral-a55` |
| `focus-ring` | `coral-200` |
| `focus-ring-neutral` | `sand-200` |
| `focus-ring-danger` | `red-300` |
| `focus-halo` | `coral-a16` |
| `danger-fill` | `red-400` |
| `danger-fill-hover` | `red-350` |
| `danger-fill-active` | `red-500` |
| `danger-tint` | `red-900` |
| `danger-tint-hover` | `red-850` |
| `danger-text` | `red-300` |
| `danger-text-on-tint` | `red-100` |
| `danger-outline` | `red-700` |
| `danger-halo` | `red-a14` |
| `danger-glow` | `red-a55` |
| `success-fill` | `mint-400` |
| `success-tint` | `mint-900` |
| `success-text` | `mint-400` |
| `success-outline` | `mint-700` |
| `success-glow` | `mint-a55` |
| `warning-fill` | `amber-400` |
| `warning-tint` | `amber-900` |
| `warning-text` | `amber-400` |
| `warning-text-on-tint` | `amber-100` |
| `warning-outline` | `amber-700` |
| `warning-halo` | `amber-a22` |
| `warning-glow` | `amber-a55` |
| `info-fill` | `sky-400` |
| `info-tint` | `sky-900` |
| `info-text` | `sky-400` |
| `info-text-on-tint` | `sky-100` |
| `info-outline` | `sky-700` |
| `status-neutral` | `sand-600` |
| `category-1` | `violet-400` |
| `category-2` | `teal-400` |
| `category-3` | `amber-400` |
| `category-4` | `coral-400` |
| `category-5` | `sky-400` |
| `category-6` | `green-300` |
| `scrim` | `sand-950-a60` |
| `shade-strong` | `black-a32` |
| `shade-soft` | `black-a18` |
| `nav-rail` | `sand-975` |
| `nav-surface` | `sand-875` |
| `nav-item-hover` | `sand-850` |
| `nav-item-active` | `sand-800` |
| `nav-item-selected` | `sand-750` |
| `nav-divider` | `sand-750` |
| `nav-text` | `sand-100` |
| `nav-text-secondary` | `sand-200` |
| `nav-text-muted` | `sand-300` |
| `nav-accent` | `coral-400` |

### `terminal`

| Key | Harbour Dark |
| --- | --- |
| `code-surface` | `sand-975` |
| `code-line-highlight` | `sand-900` |
| `code-divider` | `sand-825` |
| `code-gutter` | `sand-400` |
| `code-text` | `sand-50` |
| `code-text-muted` | `sand-200` |
| `code-selection` | `coral-850` |
| `code-match` | `amber-850` |
| `code-match-text` | `amber-100` |
| `code-match-active` | `coral-400` |
| `code-match-active-text` | `sand-950` |
| `code-caret` | `coral-400` |
| `code-error` | `red-300` |
| `code-warning` | `amber-400` |
| `code-info` | `teal-200` |
| `code-debug` | `sand-300` |
| `log-row-error` | `red-900` |
| `log-row-warn` | `amber-900` |
| `log-source-1` | `teal-200` |
| `log-source-2` | `pink-400` |
| `syntax-key` | `coral-400` |
| `syntax-string` | `lime-300` |
| `syntax-number` | `violet-400` |
| `syntax-bool` | `pink-400` |
| `syntax-punctuation` | `sand-300` |
| `syntax-comment` | `sand-400` |
| `ansi-black` | `sand-750` |
| `ansi-red` | `red-400` |
| `ansi-green` | `mint-400` |
| `ansi-yellow` | `amber-400` |
| `ansi-blue` | `sky-400` |
| `ansi-magenta` | `pink-400` |
| `ansi-cyan` | `teal-400` |
| `ansi-white` | `sand-100` |
| `ansi-bright-black` | `sand-400` |
| `ansi-bright-red` | `red-300` |
| `ansi-bright-green` | `mint-200` |
| `ansi-bright-yellow` | `amber-100` |
| `ansi-bright-blue` | `sky-300` |
| `ansi-bright-magenta` | `pink-200` |
| `ansi-bright-cyan` | `teal-200` |
| `ansi-bright-white` | `sand-50` |

### `components`

| Key | Harbour Dark |
| --- | --- |
| `well-bg` | `control-fill` |
| `field-bg` | `surface-raised` |
| `field-bg-hover` | `surface-raised-hover` |
| `row-bg` | `surface-panel` |
| `row-bg-hover` | `surface-panel-hover` |
| `btn-secondary-bg` | `control-fill-strong` |
| `btn-secondary-hover` | `control-fill-strong-hover` |
| `btn-secondary-press` | `control-fill-strong-active` |
| `pill-on-bg` | `inverse-surface` |
| `pill-on-bg-hover` | `inverse-surface-hover` |
| `pill-on-text` | `inverse-text` |
| `tooltip-bg` | `inverse-surface` |
| `tooltip-text` | `inverse-text` |
| `tooltip-key` | `inverse-text-muted` |
| `avatar-text` | `text-on-fill` |
| `av-1` | `category-1` |
| `av-2` | `category-2` |
| `av-3` | `category-3` |
| `av-4` | `category-4` |
| `av-5` | `category-5` |
| `av-6` | `category-6` |
| `key-bg` | `control-fill-strong` |
| `key-on-fill-bg` | `tint-on-fill` |
| `spinner-on-fill-track` | `track-on-fill` |
| `spinner-on-fill-head` | `text-on-fill` |
| `peek-shade-1` | `shade-strong` |
| `peek-shade-2` | `shade-soft` |
| `chart-cpu` | `accent-fill` |
| `chart-request` | `warning-text` |
| `chart-memory` | `success-fill` |
| `chart-limit` | `danger-text` |
| `chart-grid` | `divider` |

<!-- END GENERATED KEY REFERENCE -->
