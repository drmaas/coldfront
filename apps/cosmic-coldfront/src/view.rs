// SPDX-License-Identifier: GPL-3.0-only

//! Popup and panel widgets for the Coldfront applet.

use std::borrow::Cow;

use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Fill, Geometry, Path, Stroke, Style};
use cosmic::iced::widget::scrollable::{Direction as ScrollDir, Scrollbar};
use cosmic::iced::{Alignment, Color, Length, Point, Rectangle, Renderer};
use cosmic::prelude::*;
use cosmic::widget;
use cosmic::widget::svg;

use coldfront_core::{
    Day, chart_points, day_label, describe, display_icon, display_temperature, display_wind_mph,
    format_as_of, format_clock_time, format_precip_amount_mm, hour_label, is_us_zip, next_hours,
    range_placement,
};

use crate::app::{AppModel, Message, PopupTab};
use crate::fl;

const HOUR_COLUMN: f32 = 64.0;
const CHART_HEIGHT: f32 = 64.0;
const CHART_PAD: f32 = 8.0;
/// Match web `.zoom-controls` 32px buttons.
const MAP_CONTROL_SIZE: f32 = 32.0;
const RANGE_TRACK: Color = Color::from_rgb(0.902, 0.910, 0.925);
const RANGE_COOL: Color = Color::from_rgb(0.490, 0.827, 0.988);
const RANGE_WARM: Color = Color::from_rgb(0.961, 0.620, 0.043);

/// Panel temperature type role by panel size tier (matches Tempest).
fn panel_text(size: cosmic::applet::Size, label: String) -> Element<'static, Message> {
    use cosmic::applet::Size;
    use cosmic::applet::cosmic_panel_config::PanelSize;
    match size {
        Size::PanelSize(p) => match p {
            PanelSize::XL => widget::text::title1(label).into(),
            PanelSize::L => widget::text::title2(label).into(),
            PanelSize::M => widget::text::title3(label).into(),
            _ => widget::text::heading(label).into(),
        },
        _ => widget::text::heading(label).into(),
    }
}

pub fn panel(app: &AppModel) -> Element<'_, Message> {
    let is_horizontal = app.core.applet.is_horizontal();
    let suggested_size = app.core.applet.suggested_size(true);
    let applet_padding = app.core.applet.suggested_padding(true);
    let icon_size = suggested_size.0.max(12);

    let mut children: Vec<Element<'_, Message>> = Vec::new();
    if !app.alerts.is_empty() {
        children.push(
            widget::icon::from_name("dialog-warning-symbolic")
                .symbolic(true)
                .size(icon_size)
                .into(),
        );
    }
    children.push(
        widget::icon::from_name(app.panel_icon())
            .symbolic(true)
            .size(icon_size)
            .into(),
    );
    if let Some(temp) = app.panel_temperature() {
        children.push(panel_text(app.core.applet.size.clone(), temp));
    }

    let readout: Element<'_, Message> = if is_horizontal {
        widget::Row::with_children(children)
            .align_y(Alignment::Center)
            .spacing(applet_padding.1)
            .into()
    } else {
        widget::Column::with_children(children)
            .align_x(Alignment::Center)
            .spacing(applet_padding.1)
            .into()
    };

    let (horizontal_padding, vertical_padding) = if is_horizontal {
        (applet_padding.0, applet_padding.1)
    } else {
        (applet_padding.1, applet_padding.0)
    };
    let button = widget::button::custom(readout)
        .class(cosmic::theme::Button::AppletIcon)
        .padding([vertical_padding, horizontal_padding])
        .on_press(Message::TogglePopup);
    // Stable autosize id lets the panel grow past the icon slot so the
    // temperature stays visible. A fresh id each frame keeps the square.
    app.core
        .applet
        .autosize_window(app.core.applet.applet_tooltip(
            button,
            app.tooltip(),
            app.popup.is_some(),
            Message::Surface,
            app.core.main_window_id(),
        ))
        .into()
}

pub fn popup(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let mut column = widget::Column::new().spacing(spacing.space_xs);

    column = column.push(header(app));
    column = column.push(widget::divider::horizontal::default());

    let tab_control = widget::tab_bar::horizontal(&app.tab_model)
        .button_alignment(Alignment::Center)
        .on_activate(Message::TabActivated);
    column = column.push(cosmic::applet::padded_control(tab_control));

    let tab_body: Element<'_, Message> = match app.active_tab {
        PopupTab::Current => current_tab(app),
        PopupTab::Radar => radar_tab(app),
        PopupTab::Hourly => hourly_tab(app),
        PopupTab::Forecast => forecast_tab(app),
        PopupTab::Alerts => alerts_tab(app),
        PopupTab::Settings => settings_tab(app),
    };

    let tab_body: Element<'_, Message> = if app.active_tab == PopupTab::Radar {
        tab_body
    } else {
        widget::scrollable(tab_body)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    };

    column = column.push(tab_body);

    let body = widget::container(column)
        .padding([
            spacing.space_xs,
            spacing.space_l,
            spacing.space_l,
            spacing.space_l,
        ])
        .width(Length::Fixed(app.popup_width()))
        .height(Length::Fixed(app.popup_height()))
        .style(popup_fill);

    app.core
        .applet
        .popup_container(body)
        .limits(app.popup_container_limits())
        .into()
}

/// Solid popup fill. `popup_container` follows `theme.transparent`, which is
/// the frosted applet color and lets the panel show through.
fn popup_fill(theme: &cosmic::Theme) -> cosmic::widget::container::Style {
    let cosmic = theme.cosmic();
    let surface = cosmic.background(false);
    let mut fill = Color::from(surface.base);
    fill.a = 1.0;
    let mut edge = Color::from(surface.divider);
    edge.a = 1.0;
    cosmic::widget::container::Style {
        text_color: Some(surface.on.into()),
        background: Some(fill.into()),
        border: cosmic::iced::Border {
            radius: cosmic.corner_radii.radius_m.into(),
            width: 1.0,
            color: edge,
        },
        shadow: cosmic::iced::Shadow::default(),
        icon_color: Some(surface.on.into()),
        snap: true,
    }
}

fn header(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let title = match &app.place {
        Some(place) => format!("{}, {}", place.city, place.state),
        None => fl!("app-title"),
    };

    let mut meta = widget::Row::new()
        .spacing(spacing.space_xxs)
        .align_y(Alignment::Center);
    if let Some(forecast) = &app.forecast {
        meta = meta.push(widget::text::caption(fl!(
            "as-of",
            time = format_as_of(&forecast.current.time)
        )));
        if app.refresh_failed {
            meta = meta
                .push(widget::text::caption(fl!("refresh-failed")))
                .push(
                    widget::icon::from_name("dialog-warning-symbolic")
                        .size(12)
                        .symbolic(true),
                );
        }
    }

    let alerts_btn = widget::tooltip::tooltip(
        widget::button::icon(widget::icon::from_name(if app.alerts.is_empty() {
            "weather-clear-symbolic"
        } else {
            "dialog-warning-symbolic"
        }))
        .on_press(Message::SelectTab(PopupTab::Alerts))
        .padding(spacing.space_xs),
        widget::text::body(fl!("tooltip-alerts")),
        widget::tooltip::Position::Bottom,
    )
    .gap(spacing.space_xxxs);

    let settings_btn = widget::tooltip::tooltip(
        widget::button::icon(widget::icon::from_name("emblem-system-symbolic"))
            .on_press(Message::SelectTab(PopupTab::Settings))
            .padding(spacing.space_xs),
        widget::text::body(fl!("tooltip-settings")),
        widget::tooltip::Position::Bottom,
    )
    .gap(spacing.space_xxxs);

    let actions = widget::Row::new()
        .spacing(spacing.space_xxs)
        .push(alerts_btn)
        .push(settings_btn);

    widget::Column::new()
        .spacing(spacing.space_xxs)
        .push(
            widget::Row::new()
                .align_y(Alignment::Center)
                .push(widget::text::title4(title).width(Length::Fill))
                .push(actions),
        )
        .push(meta)
        .into()
}

fn current_tab(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let mut col = widget::Column::new().spacing(spacing.space_s);

    if let Some(error) = &app.map_error
        && app.forecast.is_none()
    {
        return widget::Column::new()
            .spacing(spacing.space_xs)
            .push(widget::text::title4(fl!("enter-zip")))
            .push(widget::text::body(error.clone()))
            .push(zip_row(app))
            .into();
    }

    if app.forecast.is_none() {
        if app.weather_loading {
            return widget::container(
                widget::Column::new()
                    .spacing(spacing.space_xs)
                    .align_x(Alignment::Center)
                    .push(widget::text::title4(fl!("loading"))),
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into();
        }
        return widget::Column::new()
            .spacing(spacing.space_xs)
            .push(widget::text::body(fl!("enter-zip")))
            .push(zip_row(app))
            .into();
    }

    let forecast = app.forecast.as_ref().unwrap();
    let info = describe(forecast.current.weather_code);
    let temp = display_temperature(forecast.current.temperature_c, app.config.fahrenheit);
    let high_low = forecast.daily.first().map(|day| {
        let high = display_temperature(day.temp_max_c, app.config.fahrenheit);
        let low = display_temperature(day.temp_min_c, app.config.fahrenheit);
        format!("{high}° / {low}°")
    });

    col = col.push(
        widget::Row::new()
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(weather_icon(
                display_icon(info.icon, forecast.current.is_day),
                56.0,
            ))
            .push(widget::text::title1(format!(
                "{temp}°{}",
                app.unit_suffix()
            )))
            .push(widget::text::body(info.label).width(Length::Fill).class(
                cosmic::style::Text::Color(Color::from_rgb(0.45, 0.48, 0.52)),
            )),
    );

    if let Some(high_low) = high_low {
        col = col.push(widget::text::caption(format!(
            "{} {high_low}",
            fl!("high-low")
        )));
    }

    col = col.push(widget::divider::horizontal::default());

    let feels = display_temperature(forecast.current.apparent_c, app.config.fahrenheit);
    let wind = if app.config.fahrenheit {
        format!("{} mph", display_wind_mph(forecast.current.wind_kmh))
    } else {
        format!(
            "{} km/h",
            display_temperature(forecast.current.wind_kmh, false)
        )
    };
    let precip = forecast
        .daily
        .first()
        .map(|day| format!("{}%", day.precip_probability))
        .unwrap_or_else(|| "—".into());

    col = col.push(stat_row(
        fl!("feels-like"),
        format!("{feels}°{}", app.unit_suffix()),
        fl!("humidity"),
        format!("{}%", forecast.current.humidity),
    ));
    col = col.push(stat_row(fl!("wind"), wind, fl!("precip"), precip));

    if !app.config.zip.is_empty() {
        col = col.push(widget::text::caption(fl!(
            "zip-label",
            zip = app.config.zip.as_str()
        )));
    }

    col.into()
}

fn alerts_tab(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    if app.alerts.is_empty() {
        return widget::container(
            widget::Column::new()
                .spacing(spacing.space_xs)
                .align_x(Alignment::Center)
                .push(
                    widget::icon::from_name("weather-clear-symbolic")
                        .size(48)
                        .symbolic(true),
                )
                .push(widget::text::title4(fl!("no-active-alerts")))
                .push(widget::text::caption(fl!("area-clear"))),
        )
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .into();
    }

    let mut list = widget::list_column();
    for alert in app.alerts.iter().take(3) {
        let mut item = widget::Column::new().spacing(spacing.space_xxxs).push(
            widget::Row::new()
                .spacing(spacing.space_xxs)
                .align_y(Alignment::Center)
                .push(
                    widget::icon::from_name("dialog-warning-symbolic")
                        .size(16)
                        .symbolic(true),
                )
                .push(widget::text::body(alert.event.clone())),
        );
        if !alert.headline.is_empty() {
            item = item.push(widget::text::caption(alert.headline.clone()));
        }
        list = list.add(item);
    }
    list.into()
}

fn radar_tab(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let (width, height) = app.map_size();
    let map: Element<'_, Message> = if let Some(handle) = &app.map_handle {
        widget::image(handle.clone())
            .width(Length::Fixed(width as f32))
            .height(Length::Fixed(height as f32))
            .into()
    } else if app.map_loading || app.weather_loading {
        map_placeholder(fl!("loading-radar"), width, height)
    } else if let Some(error) = &app.map_error {
        map_placeholder(error.clone(), width, height)
    } else if !is_us_zip(&app.config.zip) {
        map_placeholder(fl!("enter-zip"), width, height)
    } else {
        map_placeholder(fl!("loading-radar"), width, height)
    };

    let interaction = if app.dragging() {
        mouse::Interaction::Grabbing
    } else {
        mouse::Interaction::Grab
    };
    let map = widget::mouse_area(map)
        .on_press(Message::MapPress)
        .on_release(Message::MapRelease)
        .on_move(Message::MapMove)
        .on_exit(Message::MapLeave)
        .on_scroll(Message::MapScroll)
        .interaction(interaction);

    let expand_icon = if app.radar_expanded {
        "view-restore-symbolic"
    } else {
        "view-fullscreen-symbolic"
    };
    let expand_tip = if app.radar_expanded {
        fl!("tooltip-radar-restore")
    } else {
        fl!("tooltip-radar-expand")
    };
    let relief = app.radar_mode == crate::app::RadarMode::Relief;
    let mode_label = if relief {
        fl!("radar-mode-relief")
    } else {
        fl!("radar-mode-flat")
    };
    let mode_tip = if relief {
        fl!("tooltip-radar-flat")
    } else {
        fl!("tooltip-radar-relief")
    };

    let mode_button = widget::tooltip::tooltip(
        widget::button::text(mode_label)
            .on_press(Message::ToggleRadarMode)
            .width(Length::Fixed(MAP_CONTROL_SIZE))
            .height(Length::Fixed(MAP_CONTROL_SIZE))
            .class(if relief {
                cosmic::theme::Button::Suggested
            } else {
                map_control_button_class()
            }),
        widget::text::body(mode_tip),
        widget::tooltip::Position::Left,
    )
    .gap(4);

    let zoom_controls = widget::Column::new()
        .spacing(0)
        .width(Length::Fixed(MAP_CONTROL_SIZE))
        .push(mode_button)
        .push(widget::divider::horizontal::default())
        .push(map_button(
            "list-add-symbolic",
            Message::ZoomIn,
            fl!("tooltip-zoom-in"),
        ))
        .push(widget::divider::horizontal::default())
        .push(map_button(
            "list-remove-symbolic",
            Message::ZoomOut,
            fl!("tooltip-zoom-out"),
        ))
        .push(widget::divider::horizontal::default())
        .push(map_button(
            "find-location-symbolic",
            Message::Recenter,
            fl!("tooltip-recenter"),
        ))
        .push(widget::divider::horizontal::default())
        .push(map_button(
            expand_icon,
            Message::ToggleRadarExpand,
            expand_tip,
        ));

    let plate = widget::container(zoom_controls)
        .width(Length::Shrink)
        .style(|theme| {
            let cosmic = theme.cosmic();
            let mut style = popup_fill(theme);
            style.border = cosmic::iced::Border {
                width: 1.0,
                radius: cosmic.corner_radii.radius_s.into(),
                color: style.border.color,
            };
            style
        });

    let overlay = widget::container(plate)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(cosmic::iced::alignment::Horizontal::Right)
        .align_y(cosmic::iced::alignment::Vertical::Top)
        .padding([8, 10]);

    let stage = cosmic::iced::widget::Stack::new()
        .width(Length::Fixed(width as f32))
        .height(Length::Fixed(height as f32))
        .push(map)
        .push(overlay)
        .clip(true);

    let play = if app.playing {
        fl!("pause")
    } else {
        fl!("play")
    };
    let timeline = widget::Row::new()
        .spacing(spacing.space_xs)
        .align_y(Alignment::Center)
        .push(widget::text::caption(fl!("minutes-ago", minutes = 50i64)))
        .push(
            widget::slider(
                0..=app.frame_count(),
                app.frame_index(),
                Message::FrameSlider,
            )
            .width(Length::Fill),
        )
        .push(widget::text::caption(fl!("latest-scan")))
        .push(
            widget::button::text(if app.radar_expanded {
                fl!("radar-restore")
            } else {
                fl!("radar-expand")
            })
            .on_press(Message::ToggleRadarExpand),
        )
        .push(widget::button::text(play).on_press(Message::TogglePlay));

    let mut col = widget::Column::new()
        .spacing(spacing.space_xxs)
        .width(Length::Fill)
        .push(stage)
        .push(timeline)
        .push(widget::text::caption(format!(
            "{} · {}",
            app.current_frame_caption(),
            app.basemap_caption()
        )));

    if app.radar_mode == crate::app::RadarMode::Relief
        && app.relief_empty
        && app.map_handle.is_some()
    {
        col = col.push(widget::text::caption(fl!("radar-no-echoes")));
    }
    if app.radar_tiles == 0 && app.map_handle.is_some() {
        col = col.push(widget::text::caption(fl!("radar-missing")));
    }
    if let Some(error) = &app.map_error {
        col = col.push(widget::text::caption(error.clone()));
    }
    col.into()
}

fn hourly_tab(app: &AppModel) -> Element<'_, Message> {
    let Some(forecast) = &app.forecast else {
        let label = if app.weather_loading {
            fl!("loading")
        } else {
            fl!("enter-zip")
        };
        return widget::text::body(label).into();
    };

    let hours = next_hours(forecast, 24);
    let temps: Vec<f64> = hours
        .iter()
        .map(|hour| {
            f64::from(display_temperature(
                hour.temperature_c,
                app.config.fahrenheit,
            ))
        })
        .collect();
    let points = chart_points(
        &temps,
        f64::from(HOUR_COLUMN),
        f64::from(CHART_HEIGHT),
        f64::from(CHART_PAD),
    );
    let chart_width = HOUR_COLUMN * hours.len().max(1) as f32;
    let chart_points: Vec<Point> = points
        .iter()
        .map(|point| Point::new(point.x as f32, point.y as f32))
        .collect();

    let mut icons = widget::Row::new();
    let mut labels = widget::Row::new();
    for (index, hour) in hours.iter().enumerate() {
        let info = describe(hour.weather_code);
        let temp = temps[index] as i32;
        let pop = hour
            .precip_probability
            .map(|value| format!("{value}%"))
            .unwrap_or_else(|| " ".into());
        let low_pop = hour.precip_probability.is_some_and(|value| value <= 5);
        let mut pop_label = widget::text::caption(pop)
            .width(Length::Fixed(HOUR_COLUMN))
            .align_x(Alignment::Center);
        if low_pop {
            pop_label = pop_label.class(cosmic::style::Text::Color(Color::from_rgb(
                0.55, 0.58, 0.62,
            )));
        }
        icons = icons.push(
            widget::Column::new()
                .spacing(2)
                .align_x(Alignment::Center)
                .width(Length::Fixed(HOUR_COLUMN))
                .push(weather_icon(display_icon(info.icon, hour.is_day), 28.0))
                .push(widget::text::body(format!("{temp}°")))
                .push(pop_label),
        );
        labels = labels.push(
            widget::container(widget::text::caption(hour_label(&hour.time, index == 0)))
                .width(Length::Fixed(HOUR_COLUMN))
                .center_x(Length::Fill),
        );
    }

    let chart = widget::canvas(HourlyChart {
        points: chart_points,
    })
    .width(Length::Fixed(chart_width))
    .height(Length::Fixed(CHART_HEIGHT));

    let strip = widget::Column::new()
        .push(icons)
        .push(chart)
        .push(labels)
        .width(Length::Fixed(chart_width));

    widget::scrollable(strip)
        .direction(ScrollDir::Horizontal(Scrollbar::default()))
        .width(Length::Fill)
        .into()
}

fn forecast_tab(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let Some(forecast) = &app.forecast else {
        let label = if app.weather_loading {
            fl!("loading")
        } else {
            fl!("enter-zip")
        };
        return widget::text::body(label).into();
    };
    let today = forecast
        .current
        .time
        .get(..10)
        .unwrap_or(&forecast.current.time)
        .to_string();
    let lows: Vec<f64> = forecast
        .daily
        .iter()
        .map(|day| f64::from(display_temperature(day.temp_min_c, app.config.fahrenheit)))
        .collect();
    let highs: Vec<f64> = forecast
        .daily
        .iter()
        .map(|day| f64::from(display_temperature(day.temp_max_c, app.config.fahrenheit)))
        .collect();
    let scale_min = lows.iter().copied().fold(f64::INFINITY, f64::min);
    let scale_max = highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let selected = app.selected_forecast_day;
    let mut col = widget::Column::new().spacing(spacing.space_xxs);
    for (index, day) in forecast.daily.iter().enumerate() {
        let info = describe(day.weather_code);
        let low = lows[index] as i32;
        let high = highs[index] as i32;
        let bar = range_placement(lows[index], highs[index], scale_min, scale_max);
        let is_selected = selected == Some(index);
        let row = widget::Row::new()
            .spacing(spacing.space_xs)
            .align_y(Alignment::Center)
            .push(widget::text::body(day_label(&day.date, &today)).width(Length::Fixed(52.0)))
            .push(weather_icon(info.icon, 28.0))
            .push(widget::text::caption(info.label).width(Length::Fill).class(
                cosmic::style::Text::Color(Color::from_rgb(0.45, 0.48, 0.52)),
            ))
            .push(
                widget::text::caption(format!("{}%", day.precip_probability))
                    .width(Length::Fixed(42.0)),
            )
            .push(widget::text::caption(format!("{low}°")).width(Length::Fixed(36.0)))
            .push(
                widget::canvas(RangeBar {
                    left: bar.left as f32,
                    width: bar.width as f32,
                })
                .width(Length::FillPortion(2))
                .height(Length::Fixed(12.0)),
            )
            .push(widget::text::body(format!("{high}°")).width(Length::Fixed(36.0)));
        let mut row_wrap = widget::container(row)
            .padding([spacing.space_xxs, spacing.space_xs])
            .width(Length::Fill);
        if is_selected {
            row_wrap = row_wrap.style(|theme| {
                let cosmic = theme.cosmic();
                let mut accent = Color::from(cosmic.accent_color());
                accent.a = 0.16;
                cosmic::widget::container::Style {
                    background: Some(accent.into()),
                    border: cosmic::iced::Border {
                        width: 1.0,
                        radius: cosmic.corner_radii.radius_s.into(),
                        color: cosmic.accent_color().into(),
                    },
                    ..popup_fill(theme)
                }
            });
        }
        let button = widget::button::custom(row_wrap)
            .width(Length::Fill)
            .class(cosmic::theme::Button::Text)
            .on_press(Message::SelectForecastDay(index));
        col = col.push(button);
        if is_selected {
            col = col.push(
                widget::container(day_details_panel(
                    day,
                    &day_label(&day.date, &today),
                    app.config.fahrenheit,
                ))
                .padding([spacing.space_xs, spacing.space_s])
                .width(Length::Fill)
                .style(|theme| {
                    let cosmic = theme.cosmic();
                    let mut style = popup_fill(theme);
                    style.border = cosmic::iced::Border {
                        width: 1.0,
                        radius: cosmic.corner_radii.radius_s.into(),
                        color: style.border.color,
                    };
                    style
                }),
            );
        }
    }
    col.into()
}

fn day_details_panel(day: &Day, title: &str, fahrenheit: bool) -> Element<'static, Message> {
    let spacing = cosmic::theme::spacing();
    let info = describe(day.weather_code);
    let mut col = widget::Column::new()
        .spacing(spacing.space_xxs)
        .push(widget::text::heading(title.to_string()));
    col = col.push(detail_pair(fl!("conditions"), info.label.to_string()));
    col = col.push(detail_pair(
        fl!("precip-chance"),
        format!("{}%", day.precip_probability),
    ));
    if let Some(humidity) = day.humidity_mean {
        col = col.push(detail_pair(fl!("humidity"), format!("{humidity}%")));
    }
    if let Some(mm) = day.precip_sum_mm.filter(|value| *value > 0.0) {
        col = col.push(detail_pair(
            fl!("precip-amount"),
            format_precip_amount_mm(mm, fahrenheit),
        ));
    }
    if let Some(wind) = day.wind_max_kmh {
        let wind_text = if fahrenheit {
            format!("{} mph", display_wind_mph(wind))
        } else {
            format!("{} km/h", display_temperature(wind, false))
        };
        col = col.push(detail_pair(fl!("max-wind"), wind_text));
    }
    if let Some(sunrise) = day.sunrise.as_deref().and_then(format_clock_time) {
        col = col.push(detail_pair(fl!("sunrise"), sunrise));
    }
    if let Some(sunset) = day.sunset.as_deref().and_then(format_clock_time) {
        col = col.push(detail_pair(fl!("sunset"), sunset));
    }
    col.into()
}

fn detail_pair(label: String, value: String) -> Element<'static, Message> {
    let spacing = cosmic::theme::spacing();
    widget::Row::new()
        .spacing(spacing.space_s)
        .push(widget::text::caption(label).width(Length::Fixed(110.0)))
        .push(widget::text::body(value).width(Length::Fill))
        .into()
}

fn settings_tab(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let zip = widget::text_input(fl!("zip-placeholder"), &app.zip_draft)
        .on_input(Message::ZipDraft)
        .on_submit(|_| Message::SaveZip);
    let key = widget::text_input(fl!("maptiler-placeholder"), &app.key_draft)
        .on_input(Message::KeyDraft)
        .on_submit(|_| Message::SaveKey);

    widget::Column::new()
        .spacing(spacing.space_s)
        .push(widget::text::title4(fl!("tooltip-settings")))
        .push(
            widget::Row::new()
                .spacing(spacing.space_xs)
                .push(zip)
                .push(widget::button::suggested(fl!("save-zip")).on_press(Message::SaveZip)),
        )
        .push(widget::text::caption(fl!("units")))
        .push(
            widget::segmented_control::horizontal(&app.unit_model)
                .on_activate(Message::UnitActivated),
        )
        .push(widget::text::caption(fl!("maptiler-key")))
        .push(key)
        .push(widget::button::standard(fl!("save-key")).on_press(Message::SaveKey))
        .push(widget::settings::item(
            fl!("osm-basemap"),
            widget::toggler(app.config.allow_osm_basemap).on_toggle(Message::ToggleOsm),
        ))
        .push(widget::text::caption(fl!("osm-basemap-caption")))
        .into()
}

fn zip_row(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let zip = widget::text_input(fl!("zip-placeholder"), &app.zip_draft)
        .on_input(Message::ZipDraft)
        .on_submit(|_| Message::SaveZip);
    widget::Row::new()
        .spacing(spacing.space_xs)
        .push(zip)
        .push(widget::button::suggested(fl!("save-zip")).on_press(Message::SaveZip))
        .into()
}

fn stat_cell(label: String, value: String) -> Element<'static, Message> {
    let spacing = cosmic::theme::spacing();
    widget::Column::new()
        .spacing(spacing.space_xxxs)
        .push(widget::text::caption(label))
        .push(widget::text::heading(value))
        .width(Length::FillPortion(1))
        .into()
}

fn stat_row(
    left_label: String,
    left_value: String,
    right_label: String,
    right_value: String,
) -> Element<'static, Message> {
    widget::Row::new()
        .push(stat_cell(left_label, left_value))
        .push(stat_cell(right_label, right_value))
        .into()
}

fn map_button(icon: &'static str, message: Message, tip: String) -> Element<'static, Message> {
    widget::tooltip::tooltip(
        widget::button::icon(widget::icon::from_name(icon))
            .on_press(message)
            .width(Length::Fixed(MAP_CONTROL_SIZE))
            .height(Length::Fixed(MAP_CONTROL_SIZE))
            .padding(8)
            .class(map_control_button_class()),
        widget::text::body(tip),
        widget::tooltip::Position::Left,
    )
    .gap(4)
    .into()
}

fn map_control_button_class() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|focused, theme| map_control_style(theme, focused, false)),
        disabled: Box::new(|theme| map_control_style(theme, false, true)),
        hovered: Box::new(|focused, theme| map_control_style(theme, focused, false)),
        pressed: Box::new(|focused, theme| map_control_style(theme, focused, false)),
    }
}

fn map_control_style(
    theme: &cosmic::Theme,
    focused: bool,
    disabled: bool,
) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let surface = cosmic.background(false);
    let mut on = Color::from(surface.on);
    if disabled {
        on.a *= 0.5;
    }
    let mut style = cosmic::widget::button::Style::new();
    style.background = Some(Color::TRANSPARENT.into());
    style.icon_color = Some(on);
    style.text_color = Some(on);
    style.border_radius = cosmic.corner_radii.radius_0.into();
    if focused {
        style.outline_width = 1.0;
        style.outline_color = cosmic.accent_color().into();
        style.border_width = 2.0;
        style.border_color = Color::TRANSPARENT;
    }
    style
}

fn map_placeholder(label: String, width: u32, height: u32) -> Element<'static, Message> {
    widget::container(widget::text::body(label))
        .width(Length::Fixed(width as f32))
        .height(Length::Fixed(height as f32))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn weather_icon(name: &str, size: f32) -> Element<'static, Message> {
    let svg = weather_icon_svg(name);
    widget::svg(svg::Handle::from_memory(Cow::Owned(svg.into_bytes())))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

fn weather_icon_svg(name: &str) -> String {
    let rainy = name.contains("rain") || name == "drizzle" || name.contains("freezing");
    let snowy = name.contains("snow");
    let storm = name == "thunder";
    let night = name.ends_with("-night");
    let cloudy = name == "overcast"
        || name == "fog"
        || name == "partly-cloudy"
        || name == "partly-cloudy-night"
        || rainy
        || snowy
        || storm;
    let sun = name == "clear" || name == "mainly-clear" || name == "partly-cloudy";
    let moon =
        name == "clear-night" || name == "mainly-clear-night" || name == "partly-cloudy-night";
    let mut parts =
        String::from(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">"##);
    if sun {
        let (cx, cy) = if cloudy { (11, 12) } else { (16, 16) };
        parts.push_str(&format!(
            r##"<circle cx="{cx}" cy="{cy}" r="6" fill="#f5b942"/>"##
        ));
        if !cloudy {
            parts.push_str(
                r##"<g stroke="#f5b942" stroke-width="1.6" stroke-linecap="round"><path d="M16 4 v3 M16 25 v3 M4 16 h3 M25 16 h3 M7.5 7.5 l2 2 M22.5 22.5 l2 2 M24.5 7.5 l-2 2 M9.5 22.5 l-2 2"/></g>"##,
            );
        }
    }
    if moon {
        let path = if cloudy {
            "M14 7.5A5.5 5.5 0 1 0 14 18.5 4.5 4.5 0 1 1 14 7.5z"
        } else {
            "M20 8A7.5 7.5 0 1 0 20 23 6 6 0 1 1 20 8z"
        };
        parts.push_str(&format!(r##"<path d="{path}" fill="#cfd8e6"/>"##));
    }
    if cloudy {
        let fill = if name == "fog" {
            "#c5ccd4"
        } else if night {
            "#7a8ea3"
        } else {
            "#8aa0b8"
        };
        parts.push_str(&format!(
            r##"<path d="M10 22h13a5 5 0 0 0 .4-10 6.5 6.5 0 0 0-12.4-1.2A4.5 4.5 0 0 0 10 22z" fill="{fill}"/>"##
        ));
    }
    if rainy {
        parts.push_str(
            r##"<path d="M12 24 l-1.4 3 M17 24 l-1.4 3 M22 24 l-1.4 3" stroke="#5b8def" stroke-width="1.4" stroke-linecap="round"/>"##,
        );
    }
    if snowy {
        parts.push_str(
            r##"<path d="M13 24 v3 M17.5 24 v3 M22 24 v3 M11.8 25.2 h2.4 M16.3 25.2 h2.4 M20.8 25.2 h2.4" stroke="#d7e4f5" stroke-width="1.2" stroke-linecap="round"/>"##,
        );
    }
    if storm {
        parts.push_str(r##"<path d="M15 22 l-2 4 h3 l-2 4" fill="#f5b942"/>"##);
    }
    parts.push_str("</svg>");
    parts
}

struct HourlyChart {
    points: Vec<Point>,
}

impl canvas::Program<Message, cosmic::Theme> for HourlyChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        if self.points.len() < 2 {
            return vec![frame.into_geometry()];
        }
        let color: Color = theme.cosmic().accent_color().into();
        let path = Path::new(|builder| {
            builder.move_to(self.points[0]);
            for point in self.points.iter().skip(1) {
                builder.line_to(*point);
            }
        });
        frame.stroke(&path, Stroke::default().with_width(2.0).with_color(color));
        for point in &self.points {
            frame.fill(&Path::circle(*point, 2.5), color);
        }
        vec![frame.into_geometry()]
    }
}

struct RangeBar {
    left: f32,
    width: f32,
}

impl canvas::Program<Message, cosmic::Theme> for RangeBar {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let track = Path::rounded_rectangle(
            Point::new(0.0, (bounds.height - 6.0) / 2.0),
            cosmic::iced::Size::new(bounds.width, 6.0),
            3.0.into(),
        );
        frame.fill(&track, RANGE_TRACK);
        let left = bounds.width * (self.left / 100.0);
        let width = (bounds.width * (self.width / 100.0)).max(2.0);
        let top = (bounds.height - 6.0) / 2.0;
        let bar = Path::rounded_rectangle(
            Point::new(left, top),
            cosmic::iced::Size::new(width, 6.0),
            3.0.into(),
        );
        let gradient = canvas::gradient::Linear::new(
            Point::new(left, top + 3.0),
            Point::new(left + width, top + 3.0),
        )
        .add_stop(0.0, RANGE_COOL)
        .add_stop(1.0, RANGE_WARM);
        frame.fill(
            &bar,
            Fill {
                style: Style::Gradient(gradient.into()),
                ..Fill::default()
            },
        );
        vec![frame.into_geometry()]
    }
}
