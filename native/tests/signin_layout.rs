//! Synthetic caller measurements test logical source rules, not Material metrics
//! or rendered/device parity. 108dp intrinsic logo alone comes from source asset.
use eh_native::signin_layout::{
    Action, Dimensions, Direction, FieldMetrics, Input, Insets, LayoutError, Metrics, Plan, Rect,
    Size, WidthClass, layout, measurement_constraints,
};

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "actual {actual}, expected {expected}"
    );
}

fn input(class: WidthClass, dimensions: Dimensions) -> Input {
    Input {
        viewport: Size {
            width: 360.0,
            height: 800.0,
        },
        width_class: class,
        dimensions,
        direction: Direction::Ltr,
        insets: Insets::default(),
        scroll: 0.0,
        intrinsic_logo: Size {
            width: 108.0,
            height: 108.0,
        },
        spinner: None,
    }
}

fn metrics(input: &Input) -> Metrics {
    let cap = measurement_constraints(input).unwrap().warning_max_width;
    Metrics {
        username: FieldMetrics {
            height: 56.0,
            show_error: false,
            supporting_text_height: None,
        },
        password: FieldMetrics {
            height: 60.0,
            show_error: false,
            supporting_text_height: None,
        },
        warnings: [
            Size {
                width: cap.min(200.0),
                height: 80.0,
            },
            Size {
                width: cap.min(180.0),
                height: 40.0,
            },
        ],
        button_heights: [40.0, 44.0, 32.0, 36.0],
    }
}

fn expanded(dimensions: Dimensions) -> Input {
    let mut input = input(WidthClass::Expanded, dimensions);
    input.viewport.width = 1200.0;
    input
}

fn content_rects(plan: &Plan) -> Vec<Rect> {
    let mut rects = vec![
        plan.body,
        plan.logo,
        plan.logo_slot,
        plan.username.widget,
        plan.username.slot,
        plan.password.widget,
        plan.password.slot,
        plan.primary_row,
        plan.secondary_row,
    ];
    rects.extend(plan.caption);
    rects.extend(plan.form);
    rects.extend(plan.warnings);
    for button in plan.buttons {
        rects.extend([button.widget, button.slot]);
    }
    rects
}

#[test]
fn signin_screen_compact_medium_use_same_top_aligned_column() {
    let compact = input(WidthClass::Compact, Dimensions::DEFAULT);
    let mut medium = compact;
    medium.width_class = WidthClass::Medium;
    let measured = metrics(&compact);
    let plan = layout(&compact, &measured).unwrap();
    assert_eq!(plan, layout(&medium, &measured).unwrap());
    assert!(plan.caption.is_none());
    assert!(plan.form.is_none());
    near(plan.body.x, 16.0);
    near(plan.body.y, 16.0);
    near(plan.body.width, 328.0);
    near(plan.body.height, 768.0);
    near(plan.logo.y, 32.0); // Column remains top-aligned, not Box-centered.
    near(plan.logo.width, 108.0);
    near(plan.logo.height, 108.0);
    near(plan.logo_slot.height, 140.0);
    near(plan.username.widget.y, 156.0);
    near(plan.password.widget.y, 228.0);
    near(plan.max_scroll, 0.0);
}

#[test]
fn dimens_default_sw600_and_classes_are_independent() {
    for class in [
        WidthClass::Compact,
        WidthClass::Medium,
        WidthClass::Expanded,
    ] {
        for dims in [Dimensions::DEFAULT, Dimensions::SW600] {
            let mut input = input(class, dims);
            input.viewport.width = 1400.0;
            let widths = measurement_constraints(&input).unwrap();
            near(widths.field_width, dims.single_max_width);
            near(widths.body_width, 1400.0 - 2.0 * dims.keyline_margin);
            if class == WidthClass::Expanded {
                near(widths.caption_width.unwrap(), 480.0);
                near(widths.warning_max_width, 360.0);
            } else {
                assert!(widths.caption_width.is_none());
                near(widths.warning_max_width, dims.single_max_width);
            }
            layout(&input, &metrics(&input)).unwrap();
        }
    }
    // No expanded numeric lower bound: explicit expanded on narrow viewport clamps.
    let narrow = input(WidthClass::Expanded, Dimensions::SW600);
    let widths = measurement_constraints(&narrow).unwrap();
    near(widths.caption_width.unwrap(), 312.0);
    near(widths.form_width.unwrap(), 0.0);
    near(widths.field_width, 0.0);
}

#[test]
fn preferred_field_width_and_warning_cap_clamp_without_forcing_text_width() {
    let mut input = input(WidthClass::Compact, Dimensions::SW600);
    input.viewport.width = 300.0;
    let measured = metrics(&input);
    let plan = layout(&input, &measured).unwrap();
    near(plan.username.widget.width, 252.0); // width(480) subject to maxWidth252
    near(plan.constraints.warning_max_width, 252.0);
    near(plan.warnings[0].width, 200.0); // widthIn is not width(252)
    near(plan.warnings[0].x, 50.0);
    near(plan.logo.width, 108.0);
    near(plan.logo_slot.width, 156.0); // keyline24 on both sides
}

#[test]
fn intrinsic_logo_aspect_clamps_after_nested_padding() {
    let mut input = input(WidthClass::Compact, Dimensions::DEFAULT);
    input.viewport.width = 120.0;
    let widths = measurement_constraints(&input).unwrap();
    near(widths.body_width, 88.0);
    near(widths.logo.width, 56.0);
    near(widths.logo.height, 56.0);
    let plan = layout(&input, &metrics(&input)).unwrap();
    near(plan.logo_slot.width, 88.0);
    near(plan.logo_slot.height, 88.0);
    near(plan.logo.x, 32.0);
    input.intrinsic_logo.height = 54.0;
    near(measurement_constraints(&input).unwrap().logo.height, 28.0);
    let mut expanded = expanded(Dimensions::DEFAULT);
    expanded.viewport.width = 180.0;
    let widths = measurement_constraints(&expanded).unwrap();
    // Outer keyline32 + caption keyline32 + logo keyline32, then painter width.
    near(widths.logo.width, 84.0);
    near(widths.logo.height, 84.0);
    layout(&expanded, &metrics(&expanded)).unwrap();
}

#[test]
fn signin_screen_error_padding_uses_error_state_not_supporting_height() {
    for class in [
        WidthClass::Compact,
        WidthClass::Medium,
        WidthClass::Expanded,
    ] {
        let mut input = expanded(Dimensions::DEFAULT);
        input.width_class = class;
        input.viewport.height = 100.0; // no extra expanded min-height centering
        for username_error in [false, true] {
            for password_error in [false, true] {
                let mut measured = metrics(&input);
                measured.username.show_error = username_error;
                measured.username.supporting_text_height = username_error.then_some(0.0);
                measured.password.show_error = password_error;
                measured.password.supporting_text_height = password_error.then_some(20.0);
                let plan = layout(&input, &measured).unwrap();
                near(
                    plan.username.slot.height,
                    56.0 + if username_error { 0.0 } else { 16.0 },
                );
                near(
                    plan.password.slot.height,
                    60.0 + if password_error { 0.0 } else { 16.0 },
                );
                near(
                    plan.password.widget.y,
                    plan.username.slot.y + plan.username.slot.height,
                );
            }
        }
    }
    let input = input(WidthClass::Compact, Dimensions::DEFAULT);
    let mut measured = metrics(&input);
    measured.username.show_error = true;
    measured.username.height = 83.0; // caller-measured FULL field including support
    measured.username.supporting_text_height = Some(17.0);
    near(
        layout(&input, &measured).unwrap().username.widget.height,
        83.0,
    ); // not 100
}

#[test]
fn compact_warning_and_keyline_spacers_preserve_source_order() {
    let input = input(WidthClass::Compact, Dimensions::DEFAULT);
    let plan = layout(&input, &metrics(&input)).unwrap();
    near(
        plan.warnings[0].y - (plan.password.slot.y + plan.password.slot.height),
        24.0,
    );
    near(
        plan.warnings[1].y - (plan.warnings[0].y + plan.warnings[0].height),
        12.0,
    );
    near(
        plan.primary_row.y - (plan.warnings[1].y + plan.warnings[1].height),
        16.0,
    );
    near(plan.buttons[0].widget.y - plan.primary_row.y, 16.0);
    near(
        plan.secondary_row.y,
        plan.primary_row.y + plan.primary_row.height,
    );
}

#[test]
fn compact_weighted_buttons_preserve_register_signin_webview_guest() {
    let input = input(WidthClass::Compact, Dimensions::DEFAULT);
    let plan = layout(&input, &metrics(&input)).unwrap();
    assert_eq!(
        plan.buttons.map(|button| button.action),
        [
            Action::Register,
            Action::SignIn,
            Action::WebView,
            Action::Guest
        ]
    );
    near(plan.buttons[0].slot.width, 164.0);
    near(plan.buttons[1].slot.width, 164.0);
    near(plan.buttons[0].widget.width, 156.0);
    near(plan.buttons[0].widget.x - plan.primary_row.x, 4.0);
    near(
        plan.buttons[1].widget.x - (plan.buttons[0].widget.x + plan.buttons[0].widget.width),
        8.0,
    );
    near(plan.buttons[2].widget.width, 156.0);
    near(plan.buttons[2].widget.x - plan.secondary_row.x, 4.0);
    near(
        plan.buttons[3].widget.x - (plan.buttons[2].widget.x + plan.buttons[2].widget.width),
        8.0,
    );
    near(plan.primary_row.height, 16.0 + 44.0);
    near(plan.secondary_row.height, 36.0);
    // Default Row uses Top, not center of max child height.
    near(plan.buttons[0].widget.y, plan.buttons[1].widget.y);
    near(plan.buttons[2].widget.y, plan.buttons[3].widget.y);
}

#[test]
fn expanded_padding_before_width_gives_136_slots_and_128_widgets() {
    let input = expanded(Dimensions::SW600);
    let plan = layout(&input, &metrics(&input)).unwrap();
    assert_eq!(
        plan.buttons.map(|button| button.action),
        [
            Action::SignIn,
            Action::Register,
            Action::WebView,
            Action::Guest
        ]
    );
    near(plan.caption.unwrap().width, 480.0);
    near(plan.primary_row.width, 272.0);
    for button in plan.buttons {
        near(button.slot.width, 136.0);
        near(button.widget.width, 128.0);
        near(button.widget.x - button.slot.x, 4.0);
    }
    near(
        plan.primary_row.x,
        plan.form.unwrap().x + (plan.form.unwrap().width - 272.0) / 2.0,
    );
    near(
        plan.primary_row.y - (plan.password.slot.y + plan.password.slot.height),
        8.0,
    );
    near(
        plan.secondary_row.y - (plan.primary_row.y + plan.primary_row.height),
        8.0,
    );
    near(plan.warnings[0].y, plan.logo_slot.y + plan.logo_slot.height);
    near(
        plan.warnings[1].y,
        plan.warnings[0].y + plan.warnings[0].height,
    );
}

#[test]
fn expanded_sequential_remaining_width_clamps_second_button_not_equal_halves() {
    let mut input = expanded(Dimensions::DEFAULT);
    input.viewport.width = 712.0; // body680 - caption480 = form200
    let widths = measurement_constraints(&input).unwrap();
    for (actual, expected) in widths
        .button_widths
        .into_iter()
        .zip([128.0, 56.0, 128.0, 56.0])
    {
        near(actual, expected);
    }
    let plan = layout(&input, &metrics(&input)).unwrap();
    near(plan.primary_row.width, 200.0);
    near(plan.buttons[0].slot.width, 136.0);
    near(plan.buttons[1].slot.width, 64.0);
    near(plan.buttons[1].widget.width, 56.0);
    near(plan.username.widget.width, 200.0);
    input.viewport.width = 514.0; // form2 < padding8
    let plan = layout(&input, &metrics(&input)).unwrap();
    near(plan.buttons[0].slot.width, 2.0);
    near(plan.buttons[0].widget.width, 0.0);
    near(plan.buttons[1].slot.width, 0.0);
    near(plan.buttons[0].widget.x - plan.buttons[0].slot.x, 4.0); // padding can overflow
}

#[test]
fn compact_tiny_width_clamps_padding_and_secondary_gap() {
    let mut input = input(WidthClass::Compact, Dimensions::DEFAULT);
    input.viewport.width = 44.0; // body12, secondary padded inner4 < spacedBy8
    let plan = layout(&input, &metrics(&input)).unwrap();
    for width in plan.constraints.button_widths {
        near(width, 0.0);
    }
    near(plan.buttons[0].slot.width, 6.0);
    near(plan.buttons[2].widget.x - plan.secondary_row.x, 4.0);
    near(plan.buttons[3].widget.x - plan.buttons[2].widget.x, 4.0);
    input.viewport.width = 20.0; // outer padding overflows body zero width
    let mut plan = layout(&input, &metrics(&input)).unwrap();
    near(plan.body.width, 0.0);
    near(plan.body.x, 16.0);
    input.direction = Direction::Rtl;
    plan = layout(&input, &metrics(&input)).unwrap();
    near(plan.body.x, 4.0); // placeRelative start16 within width20
    near(plan.logo.x, -12.0); // nested logo padding remains relative, not clipped
}

#[test]
fn expanded_columns_center_in_shared_row_min_height_and_natural_height() {
    let input = expanded(Dimensions::DEFAULT);
    let measured = metrics(&input);
    let plan = layout(&input, &measured).unwrap();
    let caption = plan.caption.unwrap();
    let form = plan.form.unwrap();
    near(plan.body.height, 768.0);
    near(caption.height, 292.0); // caption32 + logo140 + warnings120
    near(form.height, 244.0); // fields72+76 + spacers16 + buttons44+36
    near(caption.y - plan.body.y, (768.0 - caption.height) / 2.0);
    near(form.y - plan.body.y, (768.0 - form.height) / 2.0);
    let mut tall_measured = measured;
    tall_measured.username.height = 900.0;
    let tall = layout(&input, &tall_measured).unwrap();
    near(tall.form.unwrap().y, tall.body.y);
    near(
        tall.caption.unwrap().y - tall.body.y,
        (tall.body.height - tall.caption.unwrap().height) / 2.0,
    );
    assert!(tall.max_scroll > 0.0);
}

#[test]
fn scroll_clamps_and_moves_system_bars_with_whole_shared_content() {
    for class in [
        WidthClass::Compact,
        WidthClass::Medium,
        WidthClass::Expanded,
    ] {
        let mut input = expanded(Dimensions::SW600);
        input.width_class = class;
        input.viewport.height = 200.0;
        input.insets = Insets {
            left: 11.0,
            top: 23.0,
            right: 31.0,
            bottom: 47.0,
        };
        let measured = metrics(&input);
        let initial = layout(&input, &measured).unwrap();
        near(initial.body.x, 35.0);
        near(initial.body.y, 47.0);
        near(
            initial.scroll_content.height,
            initial.body.height + 48.0 + 70.0,
        );
        near(initial.max_scroll, initial.scroll_content.height - 200.0);
        input.scroll = 17.0;
        let partial = layout(&input, &measured).unwrap();
        input.scroll = 100_000.0;
        let end = layout(&input, &measured).unwrap();
        near(end.scroll, initial.max_scroll);
        near(end.scroll_content.y, -initial.max_scroll);
        for ((start, partial), end) in content_rects(&initial)
            .into_iter()
            .zip(content_rects(&partial))
            .zip(content_rects(&end))
        {
            near(partial.y, start.y - 17.0);
            near(end.y, start.y - initial.max_scroll);
            near(end.x, start.x);
            near(end.width, start.width);
            near(end.height, start.height);
        }
    }
}

#[test]
fn short_content_keeps_scroll_zero_and_preserves_min_height_after_insets() {
    for class in [
        WidthClass::Compact,
        WidthClass::Medium,
        WidthClass::Expanded,
    ] {
        let mut input = expanded(Dimensions::DEFAULT);
        input.width_class = class;
        input.insets = Insets {
            left: 7.0,
            top: 13.0,
            right: 17.0,
            bottom: 29.0,
        };
        input.scroll = 500.0;
        let plan = layout(&input, &metrics(&input)).unwrap();
        near(plan.scroll, 0.0);
        near(plan.max_scroll, 0.0);
        near(plan.body.height, 800.0 - 42.0 - 32.0);
        near(plan.scroll_content.height, 800.0);
        near(plan.body.x, 23.0);
        near(plan.body.y, 29.0);
    }
}

#[test]
fn spinner_center_ignores_scroll_insets_error_and_width_class() {
    for class in [
        WidthClass::Compact,
        WidthClass::Medium,
        WidthClass::Expanded,
    ] {
        let mut input = expanded(Dimensions::DEFAULT);
        input.width_class = class;
        input.viewport.height = 180.0;
        input.insets = Insets {
            left: 100.0,
            top: 70.0,
            right: 20.0,
            bottom: 50.0,
        };
        input.spinner = Some(Size {
            width: 38.0,
            height: 42.0,
        });
        let mut measured = metrics(&input);
        let initial = layout(&input, &measured).unwrap();
        input.scroll = 1000.0;
        measured.username.show_error = true;
        measured.username.supporting_text_height = Some(12.0);
        measured.username.height = 77.0;
        let end = layout(&input, &measured).unwrap();
        assert_eq!(initial.spinner, end.spinner);
        let spinner = end.spinner.unwrap();
        near(spinner.x, 581.0);
        near(spinner.y, 69.0);
        input.spinner = None;
        let hidden = layout(&input, &measured).unwrap();
        assert!(hidden.spinner.is_none());
        assert_eq!(content_rects(&hidden), content_rects(&end));
        near(hidden.max_scroll, end.max_scroll);
    }
}

#[test]
fn rtl_mirrors_source_start_order_without_swapping_physical_system_bars() {
    for class in [
        WidthClass::Compact,
        WidthClass::Medium,
        WidthClass::Expanded,
    ] {
        for dimensions in [Dimensions::DEFAULT, Dimensions::SW600] {
            let mut input = expanded(dimensions);
            input.width_class = class;
            input.insets = Insets {
                left: 9.0,
                top: 13.0,
                right: 35.0,
                bottom: 17.0,
            };
            input.viewport.height = 240.0;
            input.scroll = 19.0;
            let measured = metrics(&input);
            let ltr = layout(&input, &measured).unwrap();
            input.direction = Direction::Rtl;
            let rtl = layout(&input, &measured).unwrap();
            near(rtl.body.x, ltr.body.x);
            near(rtl.body.x, 9.0 + dimensions.keyline_margin);
            for (left, right) in content_rects(&ltr).into_iter().zip(content_rects(&rtl)) {
                near(
                    right.x - rtl.body.x,
                    rtl.body.width - (left.x - ltr.body.x) - left.width,
                );
                near(right.y, left.y);
                near(right.width, left.width);
                near(right.height, left.height);
            }
            assert_eq!(
                ltr.buttons.map(|button| button.action),
                rtl.buttons.map(|button| button.action)
            );
            assert!(rtl.buttons[0].widget.x > rtl.buttons[1].widget.x);
            assert!(rtl.buttons[2].widget.x > rtl.buttons[3].widget.x);
            if class == WidthClass::Expanded {
                assert!(rtl.caption.unwrap().x > rtl.form.unwrap().x);
            }
        }
    }
}

#[test]
fn negative_and_nonfinite_container_inputs_reject() {
    type Mutate = fn(&mut Input, f64);
    let mutations: [Mutate; 14] = [
        |input, value| input.viewport.width = value,
        |input, value| input.viewport.height = value,
        |input, value| input.dimensions.keyline_margin = value,
        |input, value| input.dimensions.single_max_width = value,
        |input, value| input.dimensions.caption_width = value,
        |input, value| input.insets.left = value,
        |input, value| input.insets.top = value,
        |input, value| input.insets.right = value,
        |input, value| input.insets.bottom = value,
        |input, value| input.scroll = value,
        |input, value| input.intrinsic_logo.width = value,
        |input, value| input.intrinsic_logo.height = value,
        |input, value| {
            input.spinner = Some(Size {
                width: value,
                height: 1.0,
            });
        },
        |input, value| {
            input.spinner = Some(Size {
                width: 1.0,
                height: value,
            });
        },
    ];
    for mutate in mutations {
        for value in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut input = input(WidthClass::Compact, Dimensions::DEFAULT);
            mutate(&mut input, value);
            assert!(matches!(
                measurement_constraints(&input),
                Err(LayoutError::InvalidNumber(_))
            ));
        }
    }
}

#[test]
fn negative_and_nonfinite_measurements_reject() {
    type Mutate = fn(&mut Metrics, f64);
    let mutations: [Mutate; 12] = [
        |metrics, value| metrics.username.height = value,
        |metrics, value| metrics.password.height = value,
        |metrics, value| metrics.username.supporting_text_height = Some(value),
        |metrics, value| metrics.password.supporting_text_height = Some(value),
        |metrics, value| metrics.warnings[0].width = value,
        |metrics, value| metrics.warnings[0].height = value,
        |metrics, value| metrics.warnings[1].width = value,
        |metrics, value| metrics.warnings[1].height = value,
        |metrics, value| metrics.button_heights[0] = value,
        |metrics, value| metrics.button_heights[1] = value,
        |metrics, value| metrics.button_heights[2] = value,
        |metrics, value| metrics.button_heights[3] = value,
    ];
    let input = input(WidthClass::Compact, Dimensions::DEFAULT);
    for mutate in mutations {
        for value in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut measured = metrics(&input);
            mutate(&mut measured, value);
            assert!(matches!(
                layout(&input, &measured),
                Err(LayoutError::InvalidNumber(_))
            ));
        }
    }
}

#[test]
fn field_measurement_consistency_and_width_limits_reject() {
    let input = input(WidthClass::Compact, Dimensions::DEFAULT);
    let mut measured = metrics(&input);
    measured.username.show_error = true;
    assert_eq!(
        layout(&input, &measured),
        Err(LayoutError::InvalidField("username"))
    );
    measured.username.show_error = false;
    measured.username.supporting_text_height = Some(0.0);
    assert_eq!(
        layout(&input, &measured),
        Err(LayoutError::InvalidField("username"))
    );
    measured.username.show_error = true;
    measured.username.supporting_text_height = Some(57.0);
    assert_eq!(
        layout(&input, &measured),
        Err(LayoutError::InvalidField("username"))
    );
    measured = metrics(&input);
    measured.warnings[1].width = 261.0;
    assert_eq!(layout(&input, &measured), Err(LayoutError::WarningTooWide));
    let mut input = input;
    input.spinner = Some(Size {
        width: 361.0,
        height: 30.0,
    });
    assert_eq!(
        measurement_constraints(&input),
        Err(LayoutError::SpinnerTooLarge)
    );
    input.spinner = None;
    input.intrinsic_logo.width = 0.0;
    assert_eq!(
        measurement_constraints(&input),
        Err(LayoutError::UnsupportedLogoIntrinsic)
    );
}

#[test]
fn finite_inputs_overflow_reject_and_zero_allocations_remain_finite() {
    let mut input = input(WidthClass::Compact, Dimensions::DEFAULT);
    let mut measured = metrics(&input);
    measured.username.height = f64::MAX;
    measured.password.height = f64::MAX;
    assert_eq!(
        layout(&input, &measured),
        Err(LayoutError::ArithmeticOverflow)
    );
    input.insets.left = f64::MAX;
    input.insets.right = f64::MAX;
    assert_eq!(
        measurement_constraints(&input),
        Err(LayoutError::ArithmeticOverflow)
    );
    input.insets = Insets::default();
    input.dimensions.keyline_margin = f64::MAX;
    assert_eq!(
        measurement_constraints(&input),
        Err(LayoutError::ArithmeticOverflow)
    );
    input.dimensions = Dimensions::DEFAULT;
    input.viewport = Size::default();
    input.spinner = Some(Size::default());
    let plan = layout(&input, &metrics(&input)).unwrap();
    near(plan.body.width, 0.0);
    near(plan.constraints.logo.width, 0.0);
    near(plan.constraints.logo.height, 0.0);
    assert!(plan.max_scroll > 0.0); // fixed modifier padding can still overflow viewport
    for rect in content_rects(&plan) {
        assert!(rect.x.is_finite() && rect.y.is_finite());
        assert!(rect.width.is_finite() && rect.height.is_finite());
    }
}
