// SPDX-License-Identifier: GPL-3.0-only

//! Popup and panel widgets for the Coldfront applet.

use cosmic::iced::mouse;
use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::widget;

use coldfront_core::{
    day_label, describe, display_temperature, display_wind_mph, format_as_of, hour_label,
    is_us_zip, next_hours, symbolic_icon,
};

use crate::app::{AppModel, Message, PopupTab};
use crate::fl;

pub fn panel(app: &AppModel) -> Element<'_, Message> {
    let icon_size = app.core.applet.suggested_size(true).0;
    let (major, minor) = app.core.applet.suggested_padding(true);
    let (horizontal, vertical) = if app.core.applet.is_horizontal() {
        (major, minor)
    } else {
        (minor, major)
    };
    let mut content = widget::Row::new().spacing(4).align_y(Alignment::Center);
    if !app.alerts.is_empty() {
        content = content.push(
            widget::icon::from_name("dialog-warning-symbolic")
                .symbolic(true)
                .size(icon_size),
        );
    }
    content = content.push(
        widget::icon::from_name(app.panel_icon())
            .symbolic(true)
            .size(icon_size),
    );
    if let Some(temp) = app.panel_temperature() {
        content = content.push(widget::text(temp));
    }
    let button = widget::button::custom(content)
        .padding([vertical as f32, horizontal as f32])
        .on_press(Message::TogglePopup)
        .class(cosmic::theme::Button::AppletIcon);
    app.core
        .applet
        .applet_tooltip(
            button,
            app.tooltip(),
            app.popup.is_some(),
            Message::Surface,
            app.core.main_window_id(),
        )
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

    column = column.push(match app.active_tab {
        PopupTab::Current => current_tab(app),
        PopupTab::Radar => radar_tab(app),
        PopupTab::Hourly => hourly_tab(app),
        PopupTab::Forecast => forecast_tab(app),
        PopupTab::Settings => settings_tab(app),
    });

    let padded = widget::container(column).padding([
        spacing.space_xs,
        spacing.space_l,
        spacing.space_l,
        spacing.space_l,
    ]);
    let body: Element<'_, Message> = if app.active_tab == PopupTab::Radar {
        padded.into()
    } else {
        widget::scrollable(padded).height(Length::Shrink).into()
    };

    app.core
        .applet
        .popup_container(body)
        .limits(app.popup_container_limits())
        .into()
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
        .on_press(Message::SelectTab(PopupTab::Current))
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
            .push(
                widget::icon::from_name(symbolic_icon(info.icon))
                    .size(48)
                    .symbolic(true),
            )
            .push(
                widget::Column::new()
                    .spacing(spacing.space_xxxs)
                    .push(widget::text::title1(format!(
                        "{temp}°{}",
                        app.unit_suffix()
                    )))
                    .push(widget::text::body(info.label)),
            ),
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

    if !app.alerts.is_empty() {
        col = col.push(widget::divider::horizontal::default());
        col = col.push(widget::text::title4(fl!("alerts")));
        for alert in app.alerts.iter().take(3) {
            col = col.push(widget::text::body(alert.event.clone()));
            if !alert.headline.is_empty() {
                col = col.push(widget::text::caption(alert.headline.clone()));
            }
        }
    }

    col.into()
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
    } else if !is_us_zip(&app.config.zip) {
        map_placeholder(fl!("enter-zip"), width, height)
    } else {
        map_placeholder(fl!("loading-radar"), width, height)
    };

    let map = widget::mouse_area(map)
        .on_press(Message::MapPress)
        .on_release(Message::MapRelease)
        .on_move(Message::MapMove)
        .on_scroll(Message::MapScroll)
        .interaction(mouse::Interaction::Grab);

    let zoom_controls = widget::Row::new()
        .spacing(spacing.space_xxs)
        .push(
            widget::tooltip::tooltip(
                widget::button::icon(widget::icon::from_name("list-add-symbolic"))
                    .on_press(Message::ZoomIn)
                    .padding(spacing.space_xs),
                widget::text::body(fl!("tooltip-zoom-in")),
                widget::tooltip::Position::Bottom,
            )
            .gap(spacing.space_xxxs),
        )
        .push(
            widget::tooltip::tooltip(
                widget::button::icon(widget::icon::from_name("list-remove-symbolic"))
                    .on_press(Message::ZoomOut)
                    .padding(spacing.space_xs),
                widget::text::body(fl!("tooltip-zoom-out")),
                widget::tooltip::Position::Bottom,
            )
            .gap(spacing.space_xxxs),
        )
        .push(
            widget::tooltip::tooltip(
                widget::button::icon(widget::icon::from_name("find-location-symbolic"))
                    .on_press(Message::Recenter)
                    .padding(spacing.space_xs),
                widget::text::body(fl!("tooltip-recenter")),
                widget::tooltip::Position::Bottom,
            )
            .gap(spacing.space_xxxs),
        );

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
        .push(widget::button::text(play).on_press(Message::TogglePlay));

    let mut col = widget::Column::new()
        .spacing(spacing.space_xs)
        .push(map)
        .push(
            widget::Row::new()
                .align_y(Alignment::Center)
                .push(widget::text::caption(app.current_frame_label()).width(Length::Fill))
                .push(zoom_controls),
        )
        .push(timeline)
        .push(widget::text::caption(app.basemap_caption()));

    if app.radar_tiles == 0 && app.map_handle.is_some() {
        col = col.push(widget::text::caption(fl!("radar-missing")));
    }
    if let Some(error) = &app.map_error {
        col = col.push(widget::text::caption(error.clone()));
    }
    col.into()
}

fn hourly_tab(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let Some(forecast) = &app.forecast else {
        return widget::text::body(fl!("loading")).into();
    };
    let mut list = widget::list_column();
    for (index, hour) in next_hours(forecast, 24).into_iter().enumerate() {
        let info = describe(hour.weather_code);
        let temp = display_temperature(hour.temperature_c, app.config.fahrenheit);
        let row = widget::Row::new()
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(widget::text::body(hour_label(&hour.time, index == 0)).width(Length::Fixed(48.0)))
            .push(
                widget::icon::from_name(symbolic_icon(info.icon))
                    .size(20)
                    .symbolic(true),
            )
            .push(widget::text::body(format!("{temp}°")).width(Length::Fill))
            .push(widget::text::caption(format!(
                "{}%",
                hour.precip_probability
            )));
        list = list.add(row);
    }
    list.into()
}

fn forecast_tab(app: &AppModel) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let Some(forecast) = &app.forecast else {
        return widget::text::body(fl!("loading")).into();
    };
    let today = forecast
        .current
        .time
        .get(..10)
        .unwrap_or(&forecast.current.time)
        .to_string();
    let mut list = widget::list_column();
    for day in &forecast.daily {
        let info = describe(day.weather_code);
        let high = display_temperature(day.temp_max_c, app.config.fahrenheit);
        let low = display_temperature(day.temp_min_c, app.config.fahrenheit);
        let row = widget::Row::new()
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(widget::text::body(day_label(&day.date, &today)).width(Length::Fixed(64.0)))
            .push(
                widget::icon::from_name(symbolic_icon(info.icon))
                    .size(20)
                    .symbolic(true),
            )
            .push(widget::text::caption(format!("{}%", day.precip_probability)).width(Length::Fill))
            .push(widget::text::body(format!("{low}°")))
            .push(widget::text::body(format!("{high}°")));
        list = list.add(row);
    }
    widget::Column::new()
        .spacing(spacing.space_xs)
        .push(list)
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

fn map_placeholder(label: String, width: u32, height: u32) -> Element<'static, Message> {
    widget::container(widget::text::body(label))
        .width(Length::Fixed(width as f32))
        .height(Length::Fixed(height as f32))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}
