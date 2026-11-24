//! The Harbour theme: semantic colour roles and component overrides.
//!
//! Colours are defined in three layers. Each layer may only reference the
//! layer below it, and a theme (built in or loaded from a user file) may
//! change any of them:
//!
//! 1. **Palette** ([`super::palette`]): raw colour values such as `sand-950`
//!    or `coral-a16`. The only place a colour value is written.
//! 2. **Roles** ([`Role`]): colours named by purpose (`surface-panel`,
//!    `control-fill`, `text-muted`). Each role points at a palette entry.
//!    Components read roles and nothing else. Code and terminal roles
//!    (`code-*`, `log-*`, `syntax-*`, `ansi-*`) are roles too, kept in their
//!    own [`ThemeSection::Terminal`] so a terminal scheme can be overridden on
//!    its own.
//! 3. **Component overrides** ([`Override`]): optional per-component keys
//!    (`row-bg`, `pill-on-bg`, `chart-cpu`) that each point at a role.
//!
//! Shadows ([`Theme::shadow`]) are composite roles with fixed geometry whose
//! colours are palette entries; see [`super::elevation`].
//!
//! A [`ThemeDefinition`] holds the unresolved layers. Resolving it produces a
//! [`Theme`]: plain [`Color32`] fields grouped by purpose, cheap to read every
//! frame:
//!
//! ```
//! # use components::design::theme::Theme;
//! let theme = Theme::default();
//! let _fill = theme.surface.panel;
//! let _muted = theme.text.muted;
//! let _row = theme.components.row_bg;
//! let _drop = theme.shadow.raised.shadow;
//! ```
//!
//! Inside the UI, read the installed theme through [`ThemeExt::tokens`] and
//! wrap blades, dialogs, popovers, cards and similar containers in [`scope`],
//! which re-points the component overrides for that context the way the
//! design's CSS scopes do (see [`ThemeScope`]).
//!
//! User themes are TOML files listing deltas on top of one built-in theme;
//! see [`ThemeFile`]. Sizes and fonts are fixed scales, not theme keys.

mod context;
mod file;
mod scope;
mod tone;
mod visuals;

#[cfg(test)]
mod parity_tests;
#[cfg(test)]
mod reference_doc;

use std::sync::{Arc, OnceLock};

use egui::Color32;

use super::elevation::{Elevation, ShadowRole};
use super::palette::PaletteKey;

pub use context::{
    ThemeExt, current, default_theme, generation, scope, switch, sync_pixels_per_point,
};
pub use file::{EXAMPLE_THEME_FILE, ThemeError, ThemeFile, ThemeIssue};
pub use scope::ThemeScope;
pub use tone::{Tone, ToneColors};
use visuals::apply_theme;
pub use visuals::visuals;

/// Configure `ctx` with the application's default look: the built-in
/// Harbour Dark theme, egui visuals and text styles drawn from it, and the
/// application fonts.
///
/// Call this once when the egui context is created; the component test
/// harness ([`crate::test_support::setup_egui`]) uses it too.
pub fn configure_default(ctx: &egui::Context) {
    super::fonts::install(ctx);
    switch(ctx, Arc::new(Theme::default()));
}

/// The theme-file table a key belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeSection {
    /// `[palette]`: palette entries, assigned hex colours.
    Palette,
    /// `[roles]`: semantic roles, assigned palette names.
    Roles,
    /// `[terminal]`: code, log, syntax and ANSI roles, assigned palette names.
    Terminal,
    /// `[components]`: component overrides, assigned role names.
    Components,
}

impl ThemeSection {
    /// The TOML table name.
    pub const fn table(self) -> &'static str {
        match self {
            Self::Palette => "palette",
            Self::Roles => "roles",
            Self::Terminal => "terminal",
            Self::Components => "components",
        }
    }
}

impl std::fmt::Display for ThemeSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.table())
    }
}

/// Generates the role layer from one table: the [`Role`] enum with its keys,
/// section and Harbour Dark palette reference, one struct of resolved colours
/// per group, and the [`Theme`] that holds those groups.
macro_rules! roles {
    ($(
        $(#[$group_meta:meta])*
        $group:ident: $Group:ident in $section:ident {
            $( $field:ident / $variant:ident = $key:literal => $default:ident, )*
        }
    )*) => {
        /// A semantic colour role (layer 2). Roles reference palette entries.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Role {
            $($(
                #[doc = concat!("`", $key, "`")]
                $variant,
            )*)*
        }

        impl Role {
            /// Every role, in design order.
            pub const ALL: &[Self] = &[$($(Self::$variant,)*)*];
            /// The number of roles.
            pub const COUNT: usize = Self::ALL.len();

            /// The role's theme-file name, for example `"surface-panel"`.
            pub const fn key(self) -> &'static str {
                match self {
                    $($(Self::$variant => $key,)*)*
                }
            }

            /// The theme-file table the role is overridden in.
            pub const fn section(self) -> ThemeSection {
                match self {
                    $($(Self::$variant => ThemeSection::$section,)*)*
                }
            }

            /// The palette entry this role points at in Harbour Dark.
            const fn harbour_dark(self) -> PaletteKey {
                match self {
                    $($(Self::$variant => PaletteKey::$default,)*)*
                }
            }
        }

        $(
            $(#[$group_meta])*
            #[derive(Clone, Copy, Debug, PartialEq)]
            pub struct $Group {
                $(
                    #[doc = concat!("`", $key, "`")]
                    pub $field: Color32,
                )*
            }

            impl $Group {
                fn resolve(colors: &[Color32; Role::COUNT]) -> Self {
                    Self {
                        $($field: colors[Role::$variant.index()],)*
                    }
                }
            }
        )*

        /// A resolved theme: every role, override and shadow as a plain value.
        ///
        /// Roles are grouped by purpose (`theme.surface.panel`,
        /// `theme.control.fill`, `theme.code.selection`); component overrides
        /// live in [`Theme::components`]. A theme is immutable; obtain one
        /// for a context with [`Theme::scoped`] or, inside the UI, [`scope`].
        #[derive(Clone)]
        pub struct Theme {
            $(
                $(#[$group_meta])*
                pub $group: $Group,
            )*
            /// Elevation: drop shadows and hairline rings.
            pub shadow: ShadowRoles,
            /// Layer 3: component overrides, re-pointed for the current scope.
            pub components: ComponentColors,
            resolution: Resolution,
        }

        impl Theme {
            fn assemble(
                roles: &[Color32; Role::COUNT],
                shadow: ShadowRoles,
                components: ComponentColors,
                resolution: Resolution,
            ) -> Self {
                Self {
                    $($group: $Group::resolve(roles),)*
                    shadow,
                    components,
                    resolution,
                }
            }

            /// The resolved colour of `role`.
            pub fn role(&self, role: Role) -> Color32 {
                match role {
                    $($(Role::$variant => self.$group.$field,)*)*
                }
            }
        }
    };
}

roles! {
    /// Surfaces: the canvas ground, panels and raised surfaces.
    surface: SurfaceRoles in Roles {
        canvas / SurfaceCanvas = "surface-canvas" => Sand950,
        sunken / SurfaceSunken = "surface-sunken" => Sand900,
        panel / SurfacePanel = "surface-panel" => Sand875,
        panel_hover / SurfacePanelHover = "surface-panel-hover" => Sand850,
        raised / SurfaceRaised = "surface-raised" => Sand825,
        raised_hover / SurfaceRaisedHover = "surface-raised-hover" => Sand800,
    }
    /// Controls: neutral and strong control fills and borders.
    control: ControlRoles in Roles {
        fill / ControlFill = "control-fill" => Sand825,
        fill_hover / ControlFillHover = "control-fill-hover" => Sand750,
        fill_active / ControlFillActive = "control-fill-active" => Sand700,
        fill_strong / ControlFillStrong = "control-fill-strong" => Sand750,
        fill_strong_hover / ControlFillStrongHover = "control-fill-strong-hover" => Sand700,
        fill_strong_active / ControlFillStrongActive = "control-fill-strong-active" => Sand600,
        fill_disabled / ControlFillDisabled = "control-fill-disabled" => Sand900,
        border / ControlBorder = "control-border" => Sand400,
        border_hover / ControlBorderHover = "control-border-hover" => Sand300,
    }
    /// Items: highlighted and selected list, menu and table items.
    item: ItemRoles in Roles {
        highlight / ItemHighlight = "item-highlight" => Sand750,
        selected / ItemSelected = "item-selected" => Sand750,
    }
    /// Lines: dividers between regions.
    divider: DividerRoles in Roles {
        base / Divider = "divider" => Sand750,
        subtle / DividerSubtle = "divider-subtle" => Sand825,
        strong / DividerStrong = "divider-strong" => Sand700,
    }
    /// Text on surfaces and on filled controls.
    text: TextRoles in Roles {
        primary / TextPrimary = "text-primary" => Sand50,
        secondary / TextSecondary = "text-secondary" => Sand200,
        muted / TextMuted = "text-muted" => Sand300,
        disabled / TextDisabled = "text-disabled" => Sand500,
        on_fill / TextOnFill = "text-on-fill" => Sand950,
    }
    /// Inverse: light-on-dark selections and tooltips.
    inverse: InverseRoles in Roles {
        surface / InverseSurface = "inverse-surface" => Sand50,
        surface_hover / InverseSurfaceHover = "inverse-surface-hover" => Sand100,
        text / InverseText = "inverse-text" => Sand950,
        text_muted / InverseTextMuted = "inverse-text-muted" => Sand500,
    }
    /// Translucent layers drawn on top of filled controls.
    on_fill: OnFillRoles in Roles {
        tint / TintOnFill = "tint-on-fill" => Sand950A18,
        track / TrackOnFill = "track-on-fill" => Sand950A25,
    }
    /// Accent: the primary action colour and its tints.
    accent: AccentRoles in Roles {
        fill / AccentFill = "accent-fill" => Coral400,
        fill_hover / AccentFillHover = "accent-fill-hover" => Coral300,
        fill_active / AccentFillActive = "accent-fill-active" => Coral500,
        tint / AccentTint = "accent-tint" => Coral900,
        tint_hover / AccentTintHover = "accent-tint-hover" => Coral850,
        tint_active / AccentTintActive = "accent-tint-active" => Coral800,
        text / AccentText = "accent-text" => Coral400,
        outline / AccentOutline = "accent-outline" => Coral700,
        glow / AccentGlow = "accent-glow" => CoralA55,
    }
    /// Keyboard focus rings.
    focus: FocusRoles in Roles {
        ring / FocusRing = "focus-ring" => Coral200,
        ring_neutral / FocusRingNeutral = "focus-ring-neutral" => Sand200,
        ring_danger / FocusRingDanger = "focus-ring-danger" => Red300,
        halo / FocusHalo = "focus-halo" => CoralA16,
    }
    /// Status: danger.
    danger: DangerRoles in Roles {
        fill / DangerFill = "danger-fill" => Red400,
        fill_hover / DangerFillHover = "danger-fill-hover" => Red350,
        fill_active / DangerFillActive = "danger-fill-active" => Red500,
        tint / DangerTint = "danger-tint" => Red900,
        tint_hover / DangerTintHover = "danger-tint-hover" => Red850,
        text / DangerText = "danger-text" => Red300,
        text_on_tint / DangerTextOnTint = "danger-text-on-tint" => Red100,
        outline / DangerOutline = "danger-outline" => Red700,
        halo / DangerHalo = "danger-halo" => RedA14,
        glow / DangerGlow = "danger-glow" => RedA55,
    }
    /// Status: success.
    success: SuccessRoles in Roles {
        fill / SuccessFill = "success-fill" => Mint400,
        tint / SuccessTint = "success-tint" => Mint900,
        text / SuccessText = "success-text" => Mint400,
        outline / SuccessOutline = "success-outline" => Mint700,
        glow / SuccessGlow = "success-glow" => MintA55,
    }
    /// Status: warning.
    warning: WarningRoles in Roles {
        fill / WarningFill = "warning-fill" => Amber400,
        tint / WarningTint = "warning-tint" => Amber900,
        text / WarningText = "warning-text" => Amber400,
        text_on_tint / WarningTextOnTint = "warning-text-on-tint" => Amber100,
        outline / WarningOutline = "warning-outline" => Amber700,
        halo / WarningHalo = "warning-halo" => AmberA22,
        glow / WarningGlow = "warning-glow" => AmberA55,
    }
    /// Status: info.
    info: InfoRoles in Roles {
        fill / InfoFill = "info-fill" => Sky400,
        tint / InfoTint = "info-tint" => Sky900,
        text / InfoText = "info-text" => Sky400,
        text_on_tint / InfoTextOnTint = "info-text-on-tint" => Sky100,
        outline / InfoOutline = "info-outline" => Sky700,
    }
    /// Status: neutral.
    status: StatusRoles in Roles {
        neutral / StatusNeutral = "status-neutral" => Sand600,
    }
    /// Categories: avatars and resource kinds. See [`CategoryRoles::nth`].
    category: CategoryRoles in Roles {
        first / Category1 = "category-1" => Violet400,
        second / Category2 = "category-2" => Teal400,
        third / Category3 = "category-3" => Amber400,
        fourth / Category4 = "category-4" => Coral400,
        fifth / Category5 = "category-5" => Sky400,
        sixth / Category6 = "category-6" => Green300,
    }
    /// Scrims and shades laid over content.
    shade: ShadeRoles in Roles {
        scrim / Scrim = "scrim" => Sand950A60,
        strong / ShadeStrong = "shade-strong" => BlackA32,
        soft / ShadeSoft = "shade-soft" => BlackA18,
    }
    /// Navigation chrome: rail, navigator and their items.
    nav: NavRoles in Roles {
        rail / NavRail = "nav-rail" => Sand975,
        surface / NavSurface = "nav-surface" => Sand875,
        item_hover / NavItemHover = "nav-item-hover" => Sand850,
        item_active / NavItemActive = "nav-item-active" => Sand800,
        item_selected / NavItemSelected = "nav-item-selected" => Sand750,
        divider / NavDivider = "nav-divider" => Sand750,
        text / NavText = "nav-text" => Sand100,
        text_secondary / NavTextSecondary = "nav-text-secondary" => Sand200,
        text_muted / NavTextMuted = "nav-text-muted" => Sand300,
        accent / NavAccent = "nav-accent" => Coral400,
    }
    /// Code surfaces (log viewer, YAML editor) and log levels.
    code: CodeRoles in Terminal {
        surface / CodeSurface = "code-surface" => Sand975,
        line_highlight / CodeLineHighlight = "code-line-highlight" => Sand900,
        divider / CodeDivider = "code-divider" => Sand825,
        gutter / CodeGutter = "code-gutter" => Sand400,
        text / CodeText = "code-text" => Sand50,
        text_muted / CodeTextMuted = "code-text-muted" => Sand200,
        selection / CodeSelection = "code-selection" => Coral850,
        match_bg / CodeMatch = "code-match" => Amber850,
        match_text / CodeMatchText = "code-match-text" => Amber100,
        match_active_bg / CodeMatchActive = "code-match-active" => Coral400,
        match_active_text / CodeMatchActiveText = "code-match-active-text" => Sand950,
        caret / CodeCaret = "code-caret" => Coral400,
        error / CodeError = "code-error" => Red300,
        warning / CodeWarning = "code-warning" => Amber400,
        info / CodeInfo = "code-info" => Teal200,
        debug / CodeDebug = "code-debug" => Sand300,
    }
    /// Log rows and sources.
    log: LogRoles in Terminal {
        row_error / LogRowError = "log-row-error" => Red900,
        row_warn / LogRowWarn = "log-row-warn" => Amber900,
        source_1 / LogSource1 = "log-source-1" => Teal200,
        source_2 / LogSource2 = "log-source-2" => Pink400,
    }
    /// YAML syntax highlighting.
    syntax: SyntaxRoles in Terminal {
        key / SyntaxKey = "syntax-key" => Coral400,
        string / SyntaxString = "syntax-string" => Lime300,
        number / SyntaxNumber = "syntax-number" => Violet400,
        bool / SyntaxBool = "syntax-bool" => Pink400,
        punctuation / SyntaxPunctuation = "syntax-punctuation" => Sand300,
        comment / SyntaxComment = "syntax-comment" => Sand400,
    }
    /// The 16 ANSI terminal colours.
    ansi: AnsiRoles in Terminal {
        black / AnsiBlack = "ansi-black" => Sand750,
        red / AnsiRed = "ansi-red" => Red400,
        green / AnsiGreen = "ansi-green" => Mint400,
        yellow / AnsiYellow = "ansi-yellow" => Amber400,
        blue / AnsiBlue = "ansi-blue" => Sky400,
        magenta / AnsiMagenta = "ansi-magenta" => Pink400,
        cyan / AnsiCyan = "ansi-cyan" => Teal400,
        white / AnsiWhite = "ansi-white" => Sand100,
        bright_black / AnsiBrightBlack = "ansi-bright-black" => Sand400,
        bright_red / AnsiBrightRed = "ansi-bright-red" => Red300,
        bright_green / AnsiBrightGreen = "ansi-bright-green" => Mint200,
        bright_yellow / AnsiBrightYellow = "ansi-bright-yellow" => Amber100,
        bright_blue / AnsiBrightBlue = "ansi-bright-blue" => Sky300,
        bright_magenta / AnsiBrightMagenta = "ansi-bright-magenta" => Pink200,
        bright_cyan / AnsiBrightCyan = "ansi-bright-cyan" => Teal200,
        bright_white / AnsiBrightWhite = "ansi-bright-white" => Sand50,
    }
}

impl Role {
    /// Look up a role by its theme-file name.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|role| role.key() == key)
    }

    /// Position of this role in [`Role::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl CategoryRoles {
    /// The categories in order, for cycling through by index.
    pub const fn all(&self) -> [Color32; 6] {
        [
            self.first,
            self.second,
            self.third,
            self.fourth,
            self.fifth,
            self.sixth,
        ]
    }

    /// The category colour for `index`, wrapping around after six.
    pub const fn nth(&self, index: usize) -> Color32 {
        self.all()[index % 6]
    }
}

/// Generates the component override layer from one table: the [`Override`]
/// enum with its keys and default roles, and [`ComponentColors`].
macro_rules! overrides {
    ($( $field:ident / $variant:ident = $key:literal => $default:ident, )*) => {
        /// A component override (layer 3). Each override points at a role.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Override {
            $(
                #[doc = concat!("`", $key, "`")]
                $variant,
            )*
        }

        impl Override {
            /// Every override, in design order.
            pub const ALL: &[Self] = &[$(Self::$variant,)*];
            /// The number of overrides.
            pub const COUNT: usize = Self::ALL.len();

            /// The override's theme-file name, for example `"row-bg"`.
            pub const fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)*
                }
            }

            /// The role this override points at in Harbour Dark.
            const fn harbour_dark(self) -> Role {
                match self {
                    $(Self::$variant => Role::$default,)*
                }
            }
        }

        /// Resolved component overrides.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct ComponentColors {
            $(
                #[doc = concat!("`", $key, "`")]
                pub $field: Color32,
            )*
        }

        impl ComponentColors {
            fn resolve(refs: &[Role; Override::COUNT], role: impl Fn(Role) -> Color32) -> Self {
                Self::from_fn(|component| role(refs[component.index()]))
            }

            fn from_fn(color: impl Fn(Override) -> Color32) -> Self {
                Self {
                    $($field: color(Override::$variant),)*
                }
            }

            fn set(&mut self, component: Override, color: Color32) {
                match component {
                    $(Override::$variant => self.$field = color,)*
                }
            }

            /// The resolved colour of `component`.
            pub fn get(&self, component: Override) -> Color32 {
                match component {
                    $(Override::$variant => self.$field,)*
                }
            }
        }
    };
}

overrides! {
    // Chips, neutral pills, trays, skeletons.
    well_bg / WellBg = "well-bg" => ControlFill,
    // Text inputs, selects, text areas.
    field_bg / FieldBg = "field-bg" => SurfaceRaised,
    field_bg_hover / FieldBgHover = "field-bg-hover" => SurfaceRaisedHover,
    // Table row tiles.
    row_bg / RowBg = "row-bg" => SurfacePanel,
    row_bg_hover / RowBgHover = "row-bg-hover" => SurfacePanelHover,
    // Secondary and quiet danger buttons.
    btn_secondary_bg / BtnSecondaryBg = "btn-secondary-bg" => ControlFillStrong,
    btn_secondary_hover / BtnSecondaryHover = "btn-secondary-hover" => ControlFillStrongHover,
    btn_secondary_press / BtnSecondaryPress = "btn-secondary-press" => ControlFillStrongActive,
    // Selected toggle, segment and tab.
    pill_on_bg / PillOnBg = "pill-on-bg" => InverseSurface,
    pill_on_bg_hover / PillOnBgHover = "pill-on-bg-hover" => InverseSurfaceHover,
    pill_on_text / PillOnText = "pill-on-text" => InverseText,
    // Tooltips.
    tooltip_bg / TooltipBg = "tooltip-bg" => InverseSurface,
    tooltip_text / TooltipText = "tooltip-text" => InverseText,
    tooltip_key / TooltipKey = "tooltip-key" => InverseTextMuted,
    // Cluster avatars.
    avatar_text / AvatarText = "avatar-text" => TextOnFill,
    av_1 / Av1 = "av-1" => Category1,
    av_2 / Av2 = "av-2" => Category2,
    av_3 / Av3 = "av-3" => Category3,
    av_4 / Av4 = "av-4" => Category4,
    av_5 / Av5 = "av-5" => Category5,
    av_6 / Av6 = "av-6" => Category6,
    // Keycaps.
    key_bg / KeyBg = "key-bg" => ControlFillStrong,
    key_on_fill_bg / KeyOnFillBg = "key-on-fill-bg" => TintOnFill,
    // Spinner on filled buttons.
    spinner_on_fill_track / SpinnerOnFillTrack = "spinner-on-fill-track" => TrackOnFill,
    spinner_on_fill_head / SpinnerOnFillHead = "spinner-on-fill-head" => TextOnFill,
    // Usage charts.
    chart_cpu / ChartCpu = "chart-cpu" => AccentFill,
    chart_request / ChartRequest = "chart-request" => WarningText,
    chart_memory / ChartMemory = "chart-memory" => SuccessFill,
    chart_limit / ChartLimit = "chart-limit" => DangerText,
    chart_grid / ChartGrid = "chart-grid" => Divider,
}

impl Override {
    /// Look up an override by its theme-file name.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|component| component.key() == key)
    }

    /// Position of this override in [`Override::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl ComponentColors {
    /// The cluster avatar colours in order.
    pub const fn avatars(&self) -> [Color32; 6] {
        [
            self.av_1, self.av_2, self.av_3, self.av_4, self.av_5, self.av_6,
        ]
    }
}

/// Resolved elevation levels. See [`super::elevation`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowRoles {
    /// `shadow-raised`: popovers, menus, toasts, floating toolbars.
    pub raised: ShadowRole,
    /// `shadow-overlay`: blades, dialogs, command palette.
    pub overlay: ShadowRole,
    /// `shadow-tooltip`: tooltips.
    pub tooltip: ShadowRole,
    /// `shadow-drag`: dragged rows and tiles.
    pub drag: ShadowRole,
}

impl ShadowRoles {
    fn resolve(palette: impl Fn(PaletteKey) -> Color32 + Copy) -> Self {
        let level = |elevation: Elevation| elevation.spec().resolve(palette);
        Self {
            raised: level(Elevation::Raised),
            overlay: level(Elevation::Overlay),
            tooltip: level(Elevation::Tooltip),
            drag: level(Elevation::Drag),
        }
    }

    /// The resolved shadow of `elevation`.
    pub fn get(&self, elevation: Elevation) -> ShadowRole {
        match elevation {
            Elevation::Raised => self.raised,
            Elevation::Overlay => self.overlay,
            Elevation::Tooltip => self.tooltip,
            Elevation::Drag => self.drag,
        }
    }
}

/// A theme shipped with the application. User theme files extend exactly
/// one of these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BuiltinTheme {
    /// Harbour Dark: warm dark ground, soft raised surfaces, coral accent.
    #[default]
    HarbourDark,
}

impl BuiltinTheme {
    /// Every built-in theme.
    pub const ALL: &[Self] = &[Self::HarbourDark];

    /// The name theme files use in `extends`, for example `"harbour-dark"`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::HarbourDark => "harbour-dark",
        }
    }

    /// The human-readable theme name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::HarbourDark => "Harbour Dark",
        }
    }

    /// Whether the theme is drawn on a dark ground.
    pub const fn is_dark(self) -> bool {
        match self {
            Self::HarbourDark => true,
        }
    }

    /// Look up a built-in theme by its `extends` name.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|theme| theme.key() == key)
    }

    /// The palette value of `entry` in this theme.
    const fn palette(self, entry: PaletteKey) -> Color32 {
        match self {
            Self::HarbourDark => entry.harbour(),
        }
    }

    /// The palette entry `role` points at in this theme.
    const fn role(self, role: Role) -> PaletteKey {
        match self {
            Self::HarbourDark => role.harbour_dark(),
        }
    }

    /// The role `component` points at in this theme.
    const fn component(self, component: Override) -> Role {
        match self {
            Self::HarbourDark => component.harbour_dark(),
        }
    }

    /// The unresolved layers of this theme.
    pub fn definition(self) -> ThemeDefinition {
        ThemeDefinition {
            name: self.display_name().to_owned(),
            base: self,
            palette: std::array::from_fn(|index| self.palette(PaletteKey::ALL[index])),
            roles: std::array::from_fn(|index| self.role(Role::ALL[index])),
            overrides: std::array::from_fn(|index| self.component(Override::ALL[index])),
        }
    }
}

/// The unresolved three layers of a theme: palette values, the palette entry
/// each role points at, and the role each component override points at.
///
/// Changing a palette entry changes every role (and through them every
/// override) that references it once the definition is resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeDefinition {
    name: String,
    base: BuiltinTheme,
    palette: [Color32; PaletteKey::COUNT],
    roles: [PaletteKey; Role::COUNT],
    overrides: [Role; Override::COUNT],
}

impl ThemeDefinition {
    /// The theme's display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The built-in theme this definition starts from.
    pub fn base(&self) -> BuiltinTheme {
        self.base
    }

    /// Rename the theme.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// The value of a palette entry.
    pub fn palette(&self, entry: PaletteKey) -> Color32 {
        self.palette[entry.index()]
    }

    /// Replace the value of a palette entry.
    pub fn set_palette(&mut self, entry: PaletteKey, color: Color32) {
        self.palette[entry.index()] = color;
    }

    /// The palette entry `role` points at.
    pub fn role(&self, role: Role) -> PaletteKey {
        self.roles[role.index()]
    }

    /// Point `role` at another palette entry.
    pub fn set_role(&mut self, role: Role, entry: PaletteKey) {
        self.roles[role.index()] = entry;
    }

    /// The role `component` points at.
    pub fn component(&self, component: Override) -> Role {
        self.overrides[component.index()]
    }

    /// Point `component` at another role.
    pub fn set_component(&mut self, component: Override, role: Role) {
        self.overrides[component.index()] = role;
    }

    /// Resolve the layers into a [`Theme`].
    pub fn resolve(self) -> Theme {
        let definition = Arc::new(self);
        let palette = |entry: PaletteKey| definition.palette(entry);
        let roles: [Color32; Role::COUNT] =
            std::array::from_fn(|index| palette(definition.roles[index]));
        let components =
            ComponentColors::resolve(&definition.overrides, |role| roles[role.index()]);
        let shadow = ShadowRoles::resolve(palette);
        let resolution = Resolution {
            overrides: definition.overrides,
            in_overlay: false,
            scopes: ScopeCache::default(),
            definition: Arc::clone(&definition),
        };
        Theme::assemble(&roles, shadow, components, resolution)
    }
}

impl Default for ThemeDefinition {
    fn default() -> Self {
        BuiltinTheme::default().definition()
    }
}

/// How a resolved theme came about, kept so scoped variants can be derived.
#[derive(Clone)]
struct Resolution {
    definition: Arc<ThemeDefinition>,
    /// The role each override points at after scope re-pointing.
    overrides: [Role; Override::COUNT],
    /// Whether the theme is scoped inside a blade, dialog or popover.
    in_overlay: bool,
    scopes: ScopeCache,
}

/// Lazily derived scoped variants of a theme, one per [`ThemeScope`].
#[derive(Default)]
struct ScopeCache([OnceLock<Arc<Theme>>; ThemeScope::ALL.len()]);

impl Clone for ScopeCache {
    /// A clone starts with an empty cache; entries are derived again on use.
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Theme {
    /// A built-in theme, resolved.
    pub fn builtin(theme: BuiltinTheme) -> Self {
        theme.definition().resolve()
    }

    /// Parse a TOML theme file and resolve it on top of its built-in base.
    pub fn from_file_str(source: &str) -> Result<Self, ThemeError> {
        Ok(ThemeFile::parse(source)?.definition()?.resolve())
    }

    /// The theme's display name.
    pub fn name(&self) -> &str {
        self.definition().name()
    }

    /// Whether the theme is drawn on a dark ground.
    pub fn is_dark(&self) -> bool {
        self.definition().base().is_dark()
    }

    /// The unresolved layers this theme was resolved from.
    pub fn definition(&self) -> &ThemeDefinition {
        &self.resolution.definition
    }

    /// Whether this theme is scoped inside a blade, dialog or popover.
    pub fn in_overlay(&self) -> bool {
        self.resolution.in_overlay
    }

    /// The background of a card (`.card`) in the current context.
    ///
    /// Cards sit on `surface-panel` on the canvas and step up to
    /// `surface-raised` inside blades, dialogs and popovers (`.blade .card`,
    /// `.dlg .card`, `.pop .card`), whose own background is the panel tone.
    pub fn card_fill(&self) -> Color32 {
        if self.in_overlay() {
            self.surface.raised
        } else {
            self.surface.panel
        }
    }

    /// The role `component` currently points at, after scope re-pointing.
    pub fn component_role(&self, component: Override) -> Role {
        self.resolution.overrides[component.index()]
    }

    /// This theme as seen inside `scope`.
    ///
    /// Scopes re-point component overrides at other roles, never at values,
    /// and nest: a [`ThemeScope::Card`] inside a blade, dialog or popover uses
    /// the overlay card mapping. Derived themes are cached, so calling this
    /// every frame is cheap.
    pub fn scoped(&self, scope: ThemeScope) -> Arc<Self> {
        let slot = &self.resolution.scopes.0[scope.index()];
        Arc::clone(slot.get_or_init(|| Arc::new(self.derive(scope))))
    }

    fn derive(&self, scope: ThemeScope) -> Self {
        let in_overlay = self.resolution.in_overlay;
        let mut overrides = self.resolution.overrides;
        let mut components = self.components;
        for &(component, role) in scope.repoints(in_overlay) {
            overrides[component.index()] = role;
            components.set(component, self.role(role));
        }
        Self {
            components,
            resolution: Resolution {
                definition: Arc::clone(&self.resolution.definition),
                overrides,
                in_overlay: in_overlay || scope.is_overlay(),
                scopes: ScopeCache::default(),
            },
            ..self.clone()
        }
    }
}

impl Theme {
    /// A Harbour Dark variant whose roles and overrides have the given
    /// colours, ignoring the palette. Scopes still re-point overrides at
    /// roles. Only for tests that need every role to have its own colour
    /// (see `test_support::distinct_role_theme`).
    #[doc(hidden)]
    pub fn from_colors(
        role: impl Fn(Role) -> Color32,
        component: impl Fn(Override) -> Color32,
    ) -> Self {
        let base = Self::default();
        let roles: [Color32; Role::COUNT] = std::array::from_fn(|index| role(Role::ALL[index]));
        Self::assemble(
            &roles,
            base.shadow,
            ComponentColors::from_fn(component),
            base.resolution.clone(),
        )
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::builtin(BuiltinTheme::default())
    }
}

impl std::fmt::Debug for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Theme")
            .field("name", &self.name())
            .field("in_overlay", &self.resolution.in_overlay)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::palette;

    #[test]
    fn builtin_resolves_through_all_three_layers() {
        let theme = Theme::default();
        assert_eq!(theme.name(), "Harbour Dark");
        assert!(theme.is_dark());
        assert_eq!(theme.surface.canvas, palette::SAND_950);
        assert_eq!(theme.text.muted, palette::SAND_300);
        assert_eq!(theme.ansi.bright_blue, palette::SKY_300);
        assert_eq!(theme.components.row_bg, theme.surface.panel);
        assert_eq!(theme.components.chart_grid, theme.divider.base);
        assert_eq!(theme.shadow.raised.shadow.color, palette::BLACK_A55);
        assert_eq!(theme.shadow.raised.shadow.offset, [0, 24]);
        assert_eq!(
            theme.shadow.raised.ring.map(|ring| ring.color),
            Some(palette::WHITE_A04)
        );
        assert_eq!(theme.shadow.tooltip.ring, None);
    }

    #[test]
    fn palette_change_propagates_to_roles_and_overrides() {
        let mut definition = ThemeDefinition::default();
        let replacement = palette::CORAL_500;
        definition.set_palette(PaletteKey::Sand825, replacement);
        let theme = definition.resolve();

        assert_eq!(theme.surface.raised, replacement);
        assert_eq!(theme.control.fill, replacement);
        assert_eq!(theme.divider.subtle, replacement);
        assert_eq!(theme.code.divider, replacement);
        assert_eq!(theme.components.field_bg, replacement);
        assert_eq!(theme.components.well_bg, replacement);
        assert_eq!(theme.surface.panel, palette::SAND_875);
    }

    #[test]
    fn role_and_component_lookup_match_typed_fields() {
        let theme = Theme::default();
        for &role in Role::ALL {
            assert_eq!(
                theme.role(role),
                theme.definition().palette(theme.definition().role(role))
            );
        }
        for &component in Override::ALL {
            assert_eq!(
                theme.components.get(component),
                theme.role(theme.component_role(component))
            );
        }
        assert_eq!(theme.category.nth(7), theme.category.second);
        assert_eq!(theme.components.avatars()[0], theme.category.first);
    }

    #[test]
    fn card_fill_steps_up_inside_overlays() {
        let theme = Theme::default();
        assert!(!theme.in_overlay());
        assert_eq!(theme.card_fill(), theme.surface.panel);
        for overlay in [ThemeScope::Blade, ThemeScope::Dialog, ThemeScope::Popover] {
            let scoped = theme.scoped(overlay);
            assert!(scoped.in_overlay());
            assert_eq!(scoped.card_fill(), theme.surface.raised);
        }
        assert_eq!(
            theme.scoped(ThemeScope::Sidebar).card_fill(),
            theme.surface.panel
        );
    }

    #[test]
    fn configure_default_installs_and_applies_harbour_dark() {
        let ctx = egui::Context::default();
        configure_default(&ctx);

        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(generation(&ctx), 1);
        let tokens = ctx.tokens();
        assert_eq!(tokens.name(), "Harbour Dark");
        assert_eq!(ctx.global_style().visuals.panel_fill, tokens.surface.canvas);
    }
}
