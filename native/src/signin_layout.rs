//! Source-anchored `SignIn` placement, in continuous logical dp, not rendered pixels.
//!
//! Source: `app/src/main/kotlin/com/hippo/ehviewer/ui/login/SignInScreen.kt`
//! lines 152..400 (working-tree baseline at reference HEAD
//! `cd07574903bdf83e79f28975359c34ef98908ad6`). Compact and medium both use
//! `!windowSizeClass.isExpanded`; no numeric breakpoint is inferred.
//!
//! Measurement boundary: first call [`measurement_constraints`], measure widgets
//! at those widths, then call [`layout`]. Heights include actual widget layout
//! allocation, not guessed Material minimums. Warning measurements exclude source
//! modifier padding. Field measurements include supporting area/internal gaps;
//! supporting text offset is unknown, so this module emits no supporting-text rect.
//! Insets are effective physical system-bar padding after ancestor consumption.
//!
//! Constraint semantics characterized from cached `AndroidX` Compose 1.12.0-alpha02
//! bytecode; see `native/parity/fixtures/signin/compose-semantics.md`. Original debug runtime
//! graphs bind those versions in `resolved-dependencies.json`. This is a logical
//! layout model, not a Compose measurement engine or proof of device parity.
//! Density rounding, integer weight remainders/alignment, painted/touch bounds,
//! typography, resource resolution, IME, gestures and rendering remain caller gates.

// All inline constants below anchor to SignInScreen.kt, not Material tokens.
const FIELD_BOTTOM_PADDING: f64 = 16.0; // lines 158,177: thenIf(!showError).padding(bottom)
const WARNING_TOP_PADDING: [f64; 2] = [24.0, 12.0]; // lines 229,238: compact only
const BUTTON_PADDING: f64 = 4.0; // lines 252,259,265,349,356,366,380
const ROW_GAP: f64 = 8.0; // line 266: compact secondary spacedBy; lines 344,361: spacers
const EXPANDED_BUTTON_WIDTH: f64 = 128.0; // lines 349,356,366,380: padding BEFORE width
const EXPANDED_WARNING_MAX: f64 = 360.0; // lines 330,335

/// Width class supplied by caller; resource qualifiers are independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthClass {
    /// Original compact class uses vertical Column.
    Compact,
    /// Original medium class also uses vertical Column.
    Medium,
    /// Original expanded class uses one horizontally split, vertically scrolling Row.
    Expanded,
}

/// Layout direction inherited by original Compose containers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Logical start lies at physical left.
    Ltr,
    /// Logical start lies at physical right; source composition order is unchanged.
    Rtl,
}

/// Caller-resolved resource values, independent of width class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimensions {
    /// `keyline_margin`: outer, logo and expanded-caption padding; compact spacer.
    pub keyline_margin: f64,
    /// `single_max_width`: preferred field width and compact warning width cap.
    pub single_max_width: f64,
    /// `signinscreen_landscape_caption_frame_width`: preferred caption width.
    pub caption_width: f64,
}

impl Dimensions {
    /// `values/dimens.xml` lines 19,22,23: 480,16,260 dp.
    pub const DEFAULT: Self = Self {
        keyline_margin: 16.0,
        single_max_width: 260.0,
        caption_width: 480.0,
    };

    /// `values-sw600dp/dimens.xml` lines 19,20: 24,480 dp; caption inherits 480.
    /// This preset does not select the expanded width class.
    pub const SW600: Self = Self {
        keyline_margin: 24.0,
        single_max_width: 480.0,
        caption_width: 480.0,
    };
}

/// Finite, nonnegative logical-dp size. Layout functions validate public fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// Horizontal extent.
    pub width: f64,
    /// Vertical extent.
    pub height: f64,
}

/// Physical viewport-space layout rectangle; offscreen/overflow coordinates allowed.
/// This is allocation, not painted pixels or a hit-test region.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Physical left coordinate (may be negative after RTL padding/scroll).
    pub x: f64,
    /// Top coordinate (may be negative after scroll).
    pub y: f64,
    /// Allocated width, excluding padding unless documented otherwise.
    pub width: f64,
    /// Allocated height, excluding padding unless documented otherwise.
    pub height: f64,
}

/// Effective unconsumed physical system-bar insets, not raw OS or IME insets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Physical left padding, independent of layout direction.
    pub left: f64,
    /// Top padding, inside the scrolling child.
    pub top: f64,
    /// Physical right padding, independent of layout direction.
    pub right: f64,
    /// Bottom padding, inside the scrolling child.
    pub bottom: f64,
}

/// Container inputs; supplied rather than inferred from a device or breakpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    /// Allocated outer unscrolled Box size; source scrolling child fills this size.
    pub viewport: Size,
    /// Explicit caller-selected width class.
    pub width_class: WidthClass,
    /// Effective resource values resolved by caller.
    pub dimensions: Dimensions,
    /// Inherited layout direction.
    pub direction: Direction,
    /// Effective unconsumed system-bar padding.
    pub insets: Insets,
    /// Requested nonnegative vertical scroll; clamped to computed shared range.
    pub scroll: f64,
    /// Positive intrinsic painter size. Original vector is 108×108 dp
    /// (`app/src/main/res/drawable/ic_launcher_foreground.xml` lines 19,20).
    pub intrinsic_logo: Size,
    /// Actual measured spinner allocation when visible; `None` when hidden.
    /// Must fit outer Box constraints. No Material spinner size is assumed.
    pub spinner: Option<Size>,
}

/// Constraints published before caller performs text/widget measurement.
/// Vertical children have unbounded maximum height under original vertical scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasurementConstraints {
    /// Exact available body width after system bars and outer keyline padding.
    pub body_width: f64,
    /// Preferred field width clamped by parent (`width`, not `requiredWidth`).
    pub field_width: f64,
    /// Warning maximum width; actual caller-measured text can be narrower.
    pub warning_max_width: f64,
    /// Button widget widths in source order (primary pair, then webview/guest).
    pub button_widths: [f64; 4],
    /// Expanded caption allocation including caption padding; absent otherwise.
    pub caption_width: Option<f64>,
    /// Expanded form allocation, filling width remaining after caption.
    pub form_width: Option<f64>,
    /// Logo allocation excluding its keyline padding; intrinsic aspect ratio retained.
    pub logo: Size,
}

/// A complete field measurement, not an invented Material text-field size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldMetrics {
    /// Complete widget height including supporting area and internal gaps if shown;
    /// excludes source's optional 16dp external bottom padding.
    pub height: f64,
    /// Canonical original `showUsernameError` / `showPasswordError` state.
    pub show_error: bool,
    /// Measured supporting text height, for validation/provenance only.
    /// Must be `Some` exactly when error shown, and cannot exceed widget height.
    /// Zero is valid text height and does not mean error absent.
    pub supporting_text_height: Option<f64>,
}

/// Caller-measured values at [`measurement_constraints`]'s widths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Username field allocation.
    pub username: FieldMetrics,
    /// Password field allocation.
    pub password: FieldMetrics,
    /// Warning text sizes in source order, excluding source top padding (24/12
    /// compact, none expanded). Width must not exceed published cap.
    pub warnings: [Size; 2],
    /// Full widget heights at published button widths in source order;
    /// includes actual Material layout allocation, excludes source modifier padding.
    pub button_heights: [f64; 4],
}

/// Original action identity; model changes no interactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open original registration URL.
    Register,
    /// Run original sign-in action.
    SignIn,
    /// Navigate original `WebView` sign-in destination.
    WebView,
    /// Run original guest-mode action.
    Guest,
}

/// Field allocation and external padding slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldPlacement {
    /// Complete measured field, including its supporting area.
    pub widget: Rect,
    /// Field plus optional 16dp bottom padding; no padding when error shown.
    pub slot: Rect,
}

/// Button allocation and source horizontal padding slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonPlacement {
    /// Original action; array stays in source composition order, including RTL.
    pub action: Action,
    /// Widget allocation at caller-measured height.
    pub widget: Rect,
    /// Source padding wrapper allocation; compact secondary buttons have no wrapper.
    pub slot: Rect,
}

/// One deterministic viewport-space placement plan; scroll moves all content
/// including system-bar padding together. Only spinner remains unscrolled.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Constraints used for supplied measurements.
    pub constraints: MeasurementConstraints,
    /// Outer viewport clipping boundary; no inset/IME/hit-test clipping is invented.
    pub viewport: Rect,
    /// Whole shared scrolling content including system bars and outer keyline padding.
    pub scroll_content: Rect,
    /// Original scrolling Column/Row, excluding outer keyline/system-bar padding.
    pub body: Rect,
    /// Expanded caption Column including its own keyline padding.
    pub caption: Option<Rect>,
    /// Expanded form Column; absent in compact/medium (no such source container).
    pub form: Option<Rect>,
    /// Image allocation excluding logo padding.
    pub logo: Rect,
    /// Image plus source keyline padding.
    pub logo_slot: Rect,
    /// Username widget and external padding.
    pub username: FieldPlacement,
    /// Password widget and external padding.
    pub password: FieldPlacement,
    /// Warning text allocations, excluding modifier padding.
    pub warnings: [Rect; 2],
    /// Primary Row including compact-only top keyline padding.
    pub primary_row: Rect,
    /// Secondary Row including compact-only horizontal 4dp padding.
    pub secondary_row: Rect,
    /// Source composition order: compact register/sign-in, expanded sign-in/register,
    /// then webview/guest. RTL alters physical placement, not array order.
    pub buttons: [ButtonPlacement; 4],
    /// Outer-Box-centered measured spinner when visible.
    pub spinner: Option<Rect>,
    /// Maximum shared scroll; zero when padded content fits viewport.
    pub max_scroll: f64,
    /// Requested scroll clamped to `[0, max_scroll]`.
    pub scroll: f64,
}

/// Invalid or unsupported measurement/input boundary; never silently guessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// Named input/measurement must be finite and nonnegative.
    InvalidNumber(&'static str),
    /// Field error/support state mismatch or supporting text exceeds full height.
    InvalidField(&'static str),
    /// Actual warning width exceeds its published constraint.
    WarningTooWide,
    /// Caller-supplied spinner did not fit its outer Box constraints.
    SpinnerTooLarge,
    /// Zero/unspecified intrinsic painter dimensions not modeled.
    UnsupportedLogoIntrinsic,
    /// Finite inputs overflowed a derived allocation or coordinate.
    ArithmeticOverflow,
}

fn number(value: f64, name: &'static str) -> Result<(), LayoutError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(LayoutError::InvalidNumber(name))
    }
}

fn extent(value: f64) -> Result<f64, LayoutError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(LayoutError::ArithmeticOverflow)
    }
}

fn validate_input(input: &Input) -> Result<(), LayoutError> {
    for (value, name) in [
        (input.viewport.width, "viewport.width"),
        (input.viewport.height, "viewport.height"),
        (input.dimensions.keyline_margin, "keyline_margin"),
        (input.dimensions.single_max_width, "single_max_width"),
        (input.dimensions.caption_width, "caption_width"),
        (input.insets.left, "insets.left"),
        (input.insets.top, "insets.top"),
        (input.insets.right, "insets.right"),
        (input.insets.bottom, "insets.bottom"),
        (input.scroll, "scroll"),
        (input.intrinsic_logo.width, "intrinsic_logo.width"),
        (input.intrinsic_logo.height, "intrinsic_logo.height"),
    ] {
        number(value, name)?;
    }
    if input.intrinsic_logo.width == 0.0 || input.intrinsic_logo.height == 0.0 {
        return Err(LayoutError::UnsupportedLogoIntrinsic);
    }
    if let Some(size) = input.spinner {
        number(size.width, "spinner.width")?;
        number(size.height, "spinner.height")?;
        if size.width > input.viewport.width || size.height > input.viewport.height {
            return Err(LayoutError::SpinnerTooLarge);
        }
    }
    Ok(())
}

fn expanded_slots(width: f64) -> [f64; 2] {
    // Row measures unweighted children sequentially with remaining max width.
    // Each wrapper is padding(4)+width(128)+padding(4), NOT width(128) incl padding.
    let first = width.min(EXPANDED_BUTTON_WIDTH + 2.0 * BUTTON_PADDING);
    let second = (width - first).min(EXPANDED_BUTTON_WIDTH + 2.0 * BUTTON_PADDING);
    [first, second]
}

/// Publish source-faithful logical-dp widths before caller measures wrapping.
/// `width` clamps to incoming constraints; `widthIn` is a cap, not a forced width.
/// Expanded caption consumes width first; form fills the remainder, even if zero.
///
/// # Errors
/// Rejects invalid inputs, unsupported zero logo intrinsic size, overflowing padding
/// or spinner measurement outside Box constraints.
pub fn measurement_constraints(input: &Input) -> Result<MeasurementConstraints, LayoutError> {
    validate_input(input)?;
    let keyline = input.dimensions.keyline_margin;
    let bars = extent(input.insets.left + input.insets.right)?;
    let padding = extent(2.0 * keyline)?;
    let body_width = ((input.viewport.width - bars).max(0.0) - padding).max(0.0);
    let expanded = input.width_class == WidthClass::Expanded;
    let caption_width = expanded.then(|| body_width.min(input.dimensions.caption_width));
    let form_width = caption_width.map(|caption| body_width - caption);
    let field_parent = form_width.unwrap_or(body_width);
    let warning_parent = caption_width.map_or(body_width, |width| (width - padding).max(0.0));
    let logo_parent = warning_parent;
    let logo_width = (logo_parent - padding)
        .max(0.0)
        .min(input.intrinsic_logo.width);
    let logo_height =
        extent(input.intrinsic_logo.height * (logo_width / input.intrinsic_logo.width))?;
    let button_widths = if expanded {
        let slots = expanded_slots(field_parent);
        let first = (slots[0] - 2.0 * BUTTON_PADDING).max(0.0);
        let second = (slots[1] - 2.0 * BUTTON_PADDING).max(0.0);
        [first, second, first, second]
    } else {
        let primary = (body_width / 2.0 - 2.0 * BUTTON_PADDING).max(0.0);
        let secondary = ((body_width - 2.0 * BUTTON_PADDING).max(0.0) - ROW_GAP).max(0.0) / 2.0;
        [primary, primary, secondary, secondary]
    };
    Ok(MeasurementConstraints {
        body_width,
        field_width: field_parent.min(input.dimensions.single_max_width),
        warning_max_width: warning_parent.min(if expanded {
            EXPANDED_WARNING_MAX
        } else {
            input.dimensions.single_max_width
        }),
        button_widths,
        caption_width,
        form_width,
        logo: Size {
            width: logo_width,
            height: logo_height,
        },
    })
}

fn validate_field(field: FieldMetrics, name: &'static str) -> Result<(), LayoutError> {
    number(field.height, name)?;
    if let Some(height) = field.supporting_text_height {
        number(height, "supporting_text_height")?;
        if height > field.height {
            return Err(LayoutError::InvalidField(name));
        }
    }
    if field.show_error != field.supporting_text_height.is_some() {
        return Err(LayoutError::InvalidField(name));
    }
    Ok(())
}

fn validate_metrics(metrics: &Metrics, widths: MeasurementConstraints) -> Result<(), LayoutError> {
    validate_field(metrics.username, "username")?;
    validate_field(metrics.password, "password")?;
    for warning in metrics.warnings {
        number(warning.width, "warning.width")?;
        number(warning.height, "warning.height")?;
        if warning.width > widths.warning_max_width {
            return Err(LayoutError::WarningTooWide);
        }
    }
    for height in metrics.button_heights {
        number(height, "button.height")?;
    }
    Ok(())
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn field_slot_height(field: FieldMetrics) -> f64 {
    field.height
        + if field.show_error {
            0.0
        } else {
            FIELD_BOTTOM_PADDING
        }
}

fn field_placement(field: FieldMetrics, width: f64, parent: Rect, y: f64) -> FieldPlacement {
    let x = parent.x + (parent.width - width) / 2.0;
    FieldPlacement {
        widget: rect(x, y, width, field.height),
        slot: rect(x, y, width, field_slot_height(field)),
    }
}

fn logo_placement(size: Size, keyline: f64, parent: Rect, y: f64) -> (Rect, Rect) {
    let slot_width = (size.width + 2.0 * keyline).min(parent.width);
    let x = parent.x + (parent.width - slot_width) / 2.0;
    (
        rect(x + keyline, y + keyline, size.width, size.height),
        rect(x, y, slot_width, size.height + 2.0 * keyline),
    )
}

fn buttons(
    input: &Input,
    widths: MeasurementConstraints,
    metrics: &Metrics,
    primary: Rect,
    secondary: Rect,
) -> [ButtonPlacement; 4] {
    let expanded = input.width_class == WidthClass::Expanded;
    let actions = if expanded {
        [
            Action::SignIn,
            Action::Register,
            Action::WebView,
            Action::Guest,
        ]
    } else {
        [
            Action::Register,
            Action::SignIn,
            Action::WebView,
            Action::Guest,
        ]
    };
    core::array::from_fn(|index| {
        let row = if index < 2 { primary } else { secondary };
        let second = index % 2 == 1;
        let (slot_x, slot_width, pad, y) = if expanded {
            let slots = expanded_slots(row.width);
            (
                row.x + if second { slots[0] } else { 0.0 },
                slots[usize::from(second)],
                BUTTON_PADDING,
                row.y,
            )
        } else if index < 2 {
            let slot_width = row.width / 2.0;
            (
                row.x + if second { slot_width } else { 0.0 },
                slot_width,
                BUTTON_PADDING,
                row.y + input.dimensions.keyline_margin,
            )
        } else {
            let inner = (row.width - 2.0 * BUTTON_PADDING).max(0.0);
            let gap = ROW_GAP.min(inner);
            (
                row.x
                    + BUTTON_PADDING
                    + if second {
                        widths.button_widths[index] + gap
                    } else {
                        0.0
                    },
                widths.button_widths[index],
                0.0,
                row.y,
            )
        };
        ButtonPlacement {
            action: actions[index],
            widget: rect(
                slot_x + pad,
                y,
                widths.button_widths[index],
                metrics.button_heights[index],
            ),
            slot: rect(slot_x, y, slot_width, metrics.button_heights[index]),
        }
    })
}

fn compact_plan(
    input: &Input,
    metrics: &Metrics,
    widths: MeasurementConstraints,
) -> Result<Plan, LayoutError> {
    let keyline = input.dimensions.keyline_margin;
    let width = widths.body_width;
    let body = rect(0.0, 0.0, width, 0.0);
    let (logo, logo_slot) = logo_placement(widths.logo, keyline, body, 0.0);
    let username = field_placement(metrics.username, widths.field_width, body, logo_slot.height);
    let password = field_placement(
        metrics.password,
        widths.field_width,
        body,
        username.slot.y + username.slot.height,
    );
    let first_y = password.slot.y + password.slot.height + WARNING_TOP_PADDING[0];
    let first = rect(
        (width - metrics.warnings[0].width) / 2.0,
        first_y,
        metrics.warnings[0].width,
        metrics.warnings[0].height,
    );
    let second = rect(
        (width - metrics.warnings[1].width) / 2.0,
        first_y + first.height + WARNING_TOP_PADDING[1],
        metrics.warnings[1].width,
        metrics.warnings[1].height,
    );
    // Source line 246 Spacer(keyline), then line 249 Row.padding(top=keyline).
    let primary = rect(
        0.0,
        second.y + second.height + keyline,
        width,
        keyline + metrics.button_heights[0].max(metrics.button_heights[1]),
    );
    let secondary = rect(
        0.0,
        primary.y + primary.height,
        width,
        metrics.button_heights[2].max(metrics.button_heights[3]),
    );
    let natural_height = extent(secondary.y + secondary.height)?;
    Ok(base_plan(
        widths,
        rect(0.0, 0.0, width, natural_height),
        None,
        None,
        logo,
        logo_slot,
        username,
        password,
        [first, second],
        primary,
        secondary,
        &buttons(input, widths, metrics, primary, secondary),
    ))
}

fn expanded_plan(
    input: &Input,
    metrics: &Metrics,
    widths: MeasurementConstraints,
) -> Result<Plan, LayoutError> {
    let keyline = input.dimensions.keyline_margin;
    let caption_width = widths
        .caption_width
        .ok_or(LayoutError::ArithmeticOverflow)?;
    let form_width = widths.form_width.ok_or(LayoutError::ArithmeticOverflow)?;
    let caption_inner = rect(
        keyline,
        keyline,
        (caption_width - 2.0 * keyline).max(0.0),
        0.0,
    );
    let (logo, logo_slot) = logo_placement(widths.logo, keyline, caption_inner, keyline);
    let first = rect(
        caption_inner.x + (caption_inner.width - metrics.warnings[0].width) / 2.0,
        logo_slot.y + logo_slot.height,
        metrics.warnings[0].width,
        metrics.warnings[0].height,
    );
    let second = rect(
        caption_inner.x + (caption_inner.width - metrics.warnings[1].width) / 2.0,
        first.y + first.height,
        metrics.warnings[1].width,
        metrics.warnings[1].height,
    );
    let caption_height = extent(second.y + second.height + keyline)?;
    let form = rect(caption_width, 0.0, form_width, 0.0);
    let username = field_placement(metrics.username, widths.field_width, form, 0.0);
    let password = field_placement(
        metrics.password,
        widths.field_width,
        form,
        username.slot.height,
    );
    let slots = expanded_slots(form_width);
    let row_width = slots[0] + slots[1];
    let row_x = form.x + (form_width - row_width) / 2.0;
    let primary = rect(
        row_x,
        password.slot.y + password.slot.height + ROW_GAP,
        row_width,
        metrics.button_heights[0].max(metrics.button_heights[1]),
    );
    let secondary = rect(
        row_x,
        primary.y + primary.height + ROW_GAP,
        row_width,
        metrics.button_heights[2].max(metrics.button_heights[3]),
    );
    let form_height = extent(secondary.y + secondary.height)?;
    let body = rect(0.0, 0.0, widths.body_width, caption_height.max(form_height));
    Ok(base_plan(
        widths,
        body,
        Some(rect(0.0, 0.0, caption_width, caption_height)),
        Some(rect(form.x, 0.0, form_width, form_height)),
        logo,
        logo_slot,
        username,
        password,
        [first, second],
        primary,
        secondary,
        &buttons(input, widths, metrics, primary, secondary),
    ))
}

// This private constructor keeps branch-specific geometry visibly adjacent to its
// source order. It contains no additional layout decisions.
#[allow(clippy::too_many_arguments)]
fn base_plan(
    constraints: MeasurementConstraints,
    body: Rect,
    caption: Option<Rect>,
    form: Option<Rect>,
    logo: Rect,
    logo_slot: Rect,
    username: FieldPlacement,
    password: FieldPlacement,
    warnings: [Rect; 2],
    primary_row: Rect,
    secondary_row: Rect,
    buttons: &[ButtonPlacement; 4],
) -> Plan {
    Plan {
        constraints,
        viewport: Rect::default(),
        scroll_content: Rect::default(),
        body,
        caption,
        form,
        logo,
        logo_slot,
        username,
        password,
        warnings,
        primary_row,
        secondary_row,
        buttons: *buttons,
        spinner: None,
        max_scroll: 0.0,
        scroll: 0.0,
    }
}

fn shift_y(rect: &mut Rect, offset: f64) {
    rect.y += offset;
}

fn finish_plan(input: &Input, mut plan: Plan) -> Result<Plan, LayoutError> {
    let keyline = input.dimensions.keyline_margin;
    let bars_y = extent(input.insets.top + input.insets.bottom)?;
    let min_height = (input.viewport.height - bars_y - 2.0 * keyline).max(0.0);
    plan.body.height = plan.body.height.max(min_height);
    // Row's cross-axis min-height survives verticalScroll; each column centers
    // within max(natural children heights, viewport minus nested padding).
    if let (Some(caption), Some(form)) = (plan.caption.as_mut(), plan.form.as_mut()) {
        let caption_y = (plan.body.height - caption.height) / 2.0;
        let form_y = (plan.body.height - form.height) / 2.0;
        for child in [
            caption,
            &mut plan.logo,
            &mut plan.logo_slot,
            &mut plan.warnings[0],
        ] {
            shift_y(child, caption_y);
        }
        shift_y(&mut plan.warnings[1], caption_y);
        for child in [
            form,
            &mut plan.username.widget,
            &mut plan.username.slot,
            &mut plan.password.widget,
            &mut plan.password.slot,
            &mut plan.primary_row,
            &mut plan.secondary_row,
        ] {
            shift_y(child, form_y);
        }
        for button in &mut plan.buttons {
            shift_y(&mut button.widget, form_y);
            shift_y(&mut button.slot, form_y);
        }
    }
    let content_height = extent(plan.body.height + 2.0 * keyline + bars_y)?;
    plan.max_scroll = (content_height - input.viewport.height).max(0.0);
    plan.scroll = input.scroll.min(plan.max_scroll);
    let inset_width = (input.viewport.width - input.insets.left - input.insets.right).max(0.0);
    // Insets place at physical left; ordinary padding uses placeRelative, including
    // overflowing start padding when available width is smaller than both pads.
    let x = input.insets.left
        + match input.direction {
            Direction::Ltr => keyline,
            Direction::Rtl => inset_width - plan.body.width - keyline,
        };
    let y = input.insets.top + keyline - plan.scroll;
    let body_width = plan.body.width;
    for child in [
        &mut plan.body,
        &mut plan.logo,
        &mut plan.logo_slot,
        &mut plan.username.widget,
        &mut plan.username.slot,
        &mut plan.password.widget,
        &mut plan.password.slot,
        &mut plan.primary_row,
        &mut plan.secondary_row,
    ]
    .into_iter()
    .chain(plan.caption.iter_mut())
    .chain(plan.form.iter_mut())
    .chain(plan.warnings.iter_mut())
    {
        place(child, input.direction, body_width, x, y)?;
    }
    for button in &mut plan.buttons {
        place(&mut button.widget, input.direction, body_width, x, y)?;
        place(&mut button.slot, input.direction, body_width, x, y)?;
    }
    plan.viewport = rect(0.0, 0.0, input.viewport.width, input.viewport.height);
    plan.scroll_content = rect(0.0, -plan.scroll, input.viewport.width, content_height);
    plan.spinner = input.spinner.map(|size| {
        rect(
            (input.viewport.width - size.width) / 2.0,
            (input.viewport.height - size.height) / 2.0,
            size.width,
            size.height,
        )
    });
    Ok(plan)
}

fn place(
    rect: &mut Rect,
    direction: Direction,
    body_width: f64,
    x: f64,
    y: f64,
) -> Result<(), LayoutError> {
    rect.x = x + match direction {
        Direction::Ltr => rect.x,
        Direction::Rtl => body_width - rect.x - rect.width,
    };
    rect.y += y;
    if [
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        rect.x + rect.width,
        rect.y + rect.height,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        Ok(())
    } else {
        Err(LayoutError::ArithmeticOverflow)
    }
}

/// Place original compact/medium Column or shared expanded Row, using supplied
/// measurements. Returned content rects are translated by one clamped scroll;
/// spinner is centered in unscrolled outer Box. No device or gesture proof implied.
///
/// # Errors
/// Rejects invalid inputs, inconsistent field error/support measurements, warning
/// measurements wider than published cap, unsupported logo intrinsic size,
/// oversized spinner, and derived arithmetic overflow. Measurements at wrong
/// widths cannot be detected from heights alone; caller must honor preflight widths.
pub fn layout(input: &Input, metrics: &Metrics) -> Result<Plan, LayoutError> {
    let constraints = measurement_constraints(input)?;
    validate_metrics(metrics, constraints)?;
    let plan = if input.width_class == WidthClass::Expanded {
        expanded_plan(input, metrics, constraints)?
    } else {
        compact_plan(input, metrics, constraints)?
    };
    finish_plan(input, plan)
}
