//! AllBlack11 core: config, window policy, and the snapshot file used to undo changes.
//! The Win32 tray loop lives in `platform` and is compiled only for Windows.

mod accents;
mod arch;
mod brand;
mod caption;
mod commands;
mod compat;
mod config;
mod effects;
mod explain;
mod fixapp;
mod help;
mod helper_protocol;
mod identity;
mod inject;
mod legacy_policy;
mod lkg;
mod menu_model;
mod onboarding;
mod palette;
mod policy;
mod presets;
mod recipes;
mod report;
mod rules;
mod scenes;
mod schedule;
mod shell;
mod snapshot;
mod startup;
mod target;
mod timefmt;
mod trial;
mod uitheme;
mod version;

pub use accents::{key_for as accent_key, Accents, PALETTE as ACCENT_PALETTE};
pub use arch::{classify, machine_name, ArchStatus, BUILD_MACHINE};
pub use brand::{
    credit_text, is_safe_profile_url, CREATOR, CREDIT_LINK_NAME, CREDIT_PREFIX, CREDIT_URL,
    NAME as BRAND_NAME, TAGLINE as BRAND_TAGLINE, VERIFICATION_STATUS,
};
pub use commands::{
    commands as palette_commands, search as palette_search, Action as PaletteAction,
    Command as PaletteCommand, Context as PaletteContext,
};
pub use compat::{Entry as CompatEntry, Notebook, Status as CompatStatus};
pub use config::{
    base_style, bundled_default_json, parse_config, set_enabled_flag, Backdrop, Color, Config,
    Corners, DayNight, Features, FocusSettings, LegacySettings, PartialStyle, ReduceEffects, Rule,
    Style, UiSettings, FALLBACK_ACCENT_BGR,
};
pub use effects::{focus_style, reduce_reason, reduced, Explicit, SystemState};
pub use explain::{
    attribute_support, explain, framework_hint, reason_text, AttributeSupport, Confirm,
    Explanation, Framework, FrameworkHint, Support, TitleBar,
};
pub use fixapp::{
    plan as fix_plan, receipt_lines, Action as FixAction, Context as FixContext, Level as FixLevel,
    Plan as FixPlan, Surface as FixSurface,
};
pub use help::{
    find as find_tutorial, search as search_help, steps_for as tutorial_steps,
    Capabilities as TutorialCaps, Condition as TutorialCondition, Step as TutorialStep,
    Target as TutorialTarget, Topic as HelpTopic, Tutorial, TOPICS as HELP_TOPICS, TUTORIALS,
};
pub use helper_protocol::{
    may_use_helper, parse_command as parse_helper_command, Backoff as HelperBackoff,
    Command as HelperCommand, LineBuffer, Reply as HelperReply,
};
pub use identity::{check as check_identity, Fingerprint, Identity};
pub use inject::{inject_allowed, InjectTarget};
pub use legacy_policy::{
    capability, contrast_ratio, host_kind, palette, window_verdict, Capability, HostKind, Level,
    Palette, SharedState, Tech, Verdict, PALETTES,
};
pub use lkg::{is_pre_schema_2, pick as pick_config, Picked, Source as ConfigSource};
pub use menu_model::{
    flatten as flatten_menu, Drawn as MenuDrawn, Item as MenuItem, ItemKind as MenuItemKind,
};
pub use onboarding::{
    initial as onboarding_initial, parse as onboarding_parse, to_json as onboarding_json,
    State as OnboardingState, Status as OnboardingStatus, LAST_STEP as SETUP_LAST_STEP,
    STEP_TITLES as SETUP_STEP_TITLES,
};
pub use palette::{
    check_image, derive as derive_palette, sample_points, to_scene as palette_scene,
    Palette as ImagePalette, GRID as PALETTE_GRID, MAX_FILE_BYTES as IMAGE_MAX_BYTES,
};
pub use policy::{
    decide, is_borderless_fullscreen, rect_covers_monitor, Decision, WindowFacts, WS_CHILD,
    WS_DLGFRAME, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_POPUP,
};
pub use presets::{apply as apply_preset, Preset, PRESETS};
pub use recipes::{
    apply as apply_recipe, bundled as bundled_recipes, export_current as export_recipe,
    parse_recipe, preview as preview_recipe, Change, Recipe, Trust,
};
pub use report::{redact, render as render_report, source_text, Attempt, LegacyView, Report};
pub use rules::{
    describe_conflict, find_conflicts, glob_match, rank, resolve, why_it_won, Conflict,
    ConflictKind, Criterion, Resolved, RuleMatch, Source, WindowIdentity,
};
pub use scenes::{
    active as active_scene, apply as apply_scene, catalog as scene_catalog,
    compare as compare_looks, preview as preview_scene, swatch as scene_swatch,
    Applicable as SceneLike, Look, LookBook, Scene, Swatch, MAX_LOOKS, MAX_NAME as MAX_LOOK_NAME,
    SCENE_COUNT,
};
pub use schedule::{
    millis_until_change, parse_times as parse_schedule_times, phase_at, Phase as DayPhase,
    Scheduler, Times as ScheduleTimes,
};
pub use shell::{blacken_shell_slots, ShellBackup, SHELL_SLOT_BYTES};
pub use snapshot::{format_snapshots, parse_snapshots, same_process, SnapshotFile, StoredSnapshot};
pub use startup::{
    clean as startup_clean, decide as startup_decide, format as startup_format,
    parse as startup_parse, Marker as StartupMarker, Mode as StartupMode, Reason as StartupReason,
};
pub use target::{classify_target, pe_machine, route, Route, TargetArch};
pub use timefmt::format_unix_utc;
pub use trial::{constrain as constrain_trial, TrialMachine, TRIAL_MS};
pub use uitheme::{
    accent_on, blend as blend_colors, face_candidates, high_contrast as high_contrast_palette,
    layout_scale, on_accent, palette as ui_palette, resolve as resolve_appearance, Appearance,
    Face, Mode as UiMode, Palette as UiPalette, SysColors, TextStyle, ICON_FONTS,
};
pub use uitheme::{metrics as ui_metrics, BODY, BODY_STRONG, CAPTION, SUBTITLE, TITLE};
pub use version::display_version;

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub use platform::start;
