// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Limits, Subscription, Task};
use cosmic::prelude::*;
use cosmic::widget;

use coldfront_core::{
    Alert, Forecast, HttpClient, Place, applet_radar_frames, describe, display_temperature,
    display_wind_mph, format_as_of, is_us_zip, symbolic_icon, tile_window,
};

use crate::config::{self, Config};
use crate::fl;
use crate::mapload::{
    BasemapKind, LoadedFrame, WeatherBundle, load_basemap, load_radar_frame, load_weather,
};

pub const APP_ID: &str = "io.github.drmaas.CosmicColdfront";

const MAP_ZOOM: u32 = 7;
const MAP_SPAN: u32 = 3;

pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    zip_draft: String,
    key_draft: String,
    generation: u64,
    place: Option<Place>,
    forecast: Option<Forecast>,
    alerts: Vec<Alert>,
    frames: Vec<ShownFrame>,
    shown_offset: u32,
    pending: Vec<u32>,
    grid: Option<coldfront_core::TileWindow>,
    basemap: Vec<Option<Vec<u8>>>,
    map_loading: bool,
    map_error: Option<String>,
    radar_tiles: usize,
    playing: bool,
}

#[derive(Clone, Debug)]
pub struct WeatherLoaded {
    generation: u64,
    bundle: WeatherBundle,
    grid: coldfront_core::TileWindow,
    basemap: Vec<Option<Vec<u8>>>,
    first: Option<LoadedFrame>,
    pending: Vec<u32>,
}

struct ShownFrame {
    offset_minutes: u32,
    handle: cosmic::iced::widget::image::Handle,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Surface(cosmic::surface::Action<Message>),
    UpdateConfig(Config),
    ZipDraft(String),
    SaveZip,
    KeyDraft(String),
    SaveKey,
    SetFahrenheit(bool),
    ToggleOsm(bool),
    TogglePlay,
    AdvanceRadar,
    WeatherReady(Box<WeatherLoaded>),
    WeatherFailed {
        generation: u64,
        message: String,
    },
    OlderFrame {
        generation: u64,
        frame: Box<LoadedFrame>,
    },
    FrameSkipped {
        generation: u64,
    },
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let config = config::load();
        let app = Self {
            core,
            popup: None,
            zip_draft: config.zip.clone(),
            key_draft: config.maptiler_api_key.clone(),
            config,
            generation: 0,
            place: None,
            forecast: None,
            alerts: Vec::new(),
            frames: Vec::new(),
            shown_offset: 0,
            pending: Vec::new(),
            grid: None,
            basemap: Vec::new(),
            map_loading: false,
            map_error: None,
            radar_tiles: 0,
            playing: true,
        };
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let icon_size = self.core.applet.suggested_size(true).0;
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (horizontal, vertical) = if self.core.applet.is_horizontal() {
            (major, minor)
        } else {
            (minor, major)
        };
        let mut content = widget::Row::new()
            .push(
                widget::icon::from_name(self.panel_icon())
                    .symbolic(true)
                    .size(icon_size),
            )
            .spacing(4)
            .align_y(Alignment::Center);
        if let Some(temp) = self.panel_temperature() {
            content = content.push(widget::text(temp));
        }
        let button = widget::button::custom(content)
            .padding([vertical as f32, horizontal as f32])
            .on_press(Message::TogglePopup)
            .class(cosmic::theme::Button::AppletIcon);
        self.core
            .applet
            .applet_tooltip(
                button,
                self.tooltip(),
                self.popup.is_some(),
                Message::Surface,
                self.core.main_window_id(),
            )
            .into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if self.popup != Some(id) {
            return widget::text("").into();
        }
        let content = widget::Column::new()
            .spacing(10)
            .padding(12)
            .width(Length::Fill)
            .push(self.summary())
            .push(self.map_block())
            .push(self.details())
            .push(self.alert_block())
            .push(self.settings_block());
        self.core
            .applet
            .popup_container(widget::scrollable(content))
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = vec![
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ];
        if self.popup.is_some() && self.playing && self.frames.len() > 1 {
            subscriptions.push(
                cosmic::iced::time::every(std::time::Duration::from_millis(800))
                    .map(|_| Message::AdvanceRadar),
            );
        }
        Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => return self.toggle_popup(),
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Surface(action) => return Task::done(cosmic::action::surface(action)),
            Message::UpdateConfig(config) => {
                let zip_changed = config.zip != self.config.zip;
                self.config = config;
                self.zip_draft.clone_from(&self.config.zip);
                self.key_draft.clone_from(&self.config.maptiler_api_key);
                if zip_changed && self.popup.is_some() {
                    return self.start_load();
                }
            }
            Message::ZipDraft(value) => self.zip_draft = value,
            Message::SaveZip => {
                self.config.zip = self.zip_draft.trim().to_string();
                config::save(&self.config);
                return self.start_load();
            }
            Message::KeyDraft(value) => self.key_draft = value,
            Message::SaveKey => {
                self.config.maptiler_api_key = self.key_draft.trim().to_string();
                config::save(&self.config);
                return self.start_load();
            }
            Message::SetFahrenheit(fahrenheit) => {
                self.config.fahrenheit = fahrenheit;
                config::save(&self.config);
            }
            Message::ToggleOsm(enabled) => {
                self.config.allow_osm_basemap = enabled;
                config::save(&self.config);
                if self.popup.is_some() {
                    return self.start_load();
                }
            }
            Message::TogglePlay => self.playing = !self.playing,
            Message::AdvanceRadar => self.advance_radar(),
            Message::WeatherReady(loaded) => {
                let WeatherLoaded {
                    generation,
                    bundle,
                    grid,
                    basemap,
                    first,
                    pending,
                } = *loaded;
                if generation != self.generation {
                    return Task::none();
                }
                self.place = Some(bundle.place);
                self.forecast = Some(bundle.forecast);
                self.alerts = bundle.alerts;
                self.grid = Some(grid);
                self.basemap = basemap;
                self.pending = pending;
                self.frames.clear();
                if let Some(frame) = first {
                    self.radar_tiles = frame.radar_tiles;
                    self.push_frame(frame);
                }
                self.map_loading = false;
                return self.load_next_frame();
            }
            Message::WeatherFailed {
                generation,
                message,
            } => {
                if generation == self.generation {
                    self.map_loading = false;
                    self.map_error = Some(message);
                }
            }
            Message::OlderFrame { generation, frame } => {
                if generation != self.generation {
                    return Task::none();
                }
                self.radar_tiles = self.radar_tiles.max(frame.radar_tiles);
                self.push_frame(*frame);
                return self.load_next_frame();
            }
            Message::FrameSkipped { generation } => {
                if generation == self.generation {
                    return self.load_next_frame();
                }
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl AppModel {
    fn toggle_popup(&mut self) -> Task<cosmic::Action<Message>> {
        if let Some(id) = self.popup.take() {
            return destroy_popup(id);
        }
        let Some(parent) = self.core.main_window_id() else {
            return Task::none();
        };
        let id = Id::unique();
        self.popup = Some(id);
        let mut settings = self
            .core
            .applet
            .get_popup_settings(parent, id, None, None, None);
        settings.positioner.size_limits = Limits::NONE
            .min_width(380.0)
            .max_width(448.0)
            .min_height(280.0)
            .max_height(900.0);
        let open = get_popup(settings);
        let needs_load = self.forecast.is_none() || self.frames.is_empty();
        if needs_load && is_us_zip(&self.config.zip) {
            Task::batch([open, self.start_load()])
        } else if !is_us_zip(&self.config.zip) {
            self.map_loading = false;
            open
        } else {
            open
        }
    }

    fn start_load(&mut self) -> Task<cosmic::Action<Message>> {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.frames.clear();
        self.pending.clear();
        self.map_error = None;
        self.radar_tiles = 0;
        let zip = self.config.zip.clone();
        if !is_us_zip(&zip) {
            self.map_loading = false;
            self.place = None;
            self.forecast = None;
            self.alerts.clear();
            self.map_error = Some(coldfront_core::Error::InvalidZip.to_string());
            return Task::none();
        }
        self.map_loading = true;
        let kind = basemap_kind(&self.config);
        cosmic::task::future(async move {
            let client = match HttpClient::new() {
                Ok(client) => client,
                Err(error) => {
                    return Message::WeatherFailed {
                        generation,
                        message: error.to_string(),
                    };
                }
            };
            let bundle = match load_weather(&client, &zip).await {
                Ok(bundle) => bundle,
                Err(error) => {
                    return Message::WeatherFailed {
                        generation,
                        message: error.to_string(),
                    };
                }
            };
            let grid = tile_window(
                bundle.place.latitude,
                bundle.place.longitude,
                MAP_ZOOM,
                MAP_SPAN,
                MAP_SPAN,
            );
            let basemap = load_basemap(&client, &grid, &kind).await;
            let frames = applet_radar_frames();
            let first = match frames.last() {
                Some(frame) => load_radar_frame(&client, &grid, &basemap, frame).await.ok(),
                None => None,
            };
            let pending = frames
                .iter()
                .rev()
                .skip(1)
                .map(|frame| frame.offset_minutes)
                .collect();
            Message::WeatherReady(Box::new(WeatherLoaded {
                generation,
                bundle,
                grid,
                basemap,
                first,
                pending,
            }))
        })
    }

    fn load_next_frame(&mut self) -> Task<cosmic::Action<Message>> {
        let Some(offset) = self.pending.pop() else {
            return Task::none();
        };
        let Some(grid) = self.grid else {
            return Task::none();
        };
        let Some(frame) = applet_radar_frames()
            .into_iter()
            .find(|item| item.offset_minutes == offset)
        else {
            return Task::none();
        };
        let basemap = self.basemap.clone();
        let generation = self.generation;
        cosmic::task::future(async move {
            let client = match HttpClient::new() {
                Ok(client) => client,
                Err(_) => return Message::FrameSkipped { generation },
            };
            match load_radar_frame(&client, &grid, &basemap, &frame).await {
                Ok(frame) => Message::OlderFrame {
                    generation,
                    frame: Box::new(frame),
                },
                Err(_) => Message::FrameSkipped { generation },
            }
        })
    }

    fn push_frame(&mut self, frame: LoadedFrame) {
        let offset = frame.offset_minutes;
        let handle =
            cosmic::iced::widget::image::Handle::from_rgba(frame.width, frame.height, frame.pixels);
        if let Some(existing) = self
            .frames
            .iter_mut()
            .find(|item| item.offset_minutes == offset)
        {
            existing.handle = handle;
            return;
        }
        self.frames.push(ShownFrame {
            offset_minutes: offset,
            handle,
        });
        self.frames.sort_by_key(|item| item.offset_minutes);
        if self.frames.len() == 1 {
            self.shown_offset = offset;
        }
    }

    fn advance_radar(&mut self) {
        if self.frames.len() < 2 {
            return;
        }
        let index = self
            .frames
            .iter()
            .position(|frame| frame.offset_minutes == self.shown_offset)
            .unwrap_or(0);
        let next = (index + 1) % self.frames.len();
        self.shown_offset = self.frames[next].offset_minutes;
    }

    fn current_frame(&self) -> Option<&ShownFrame> {
        self.frames
            .iter()
            .find(|frame| frame.offset_minutes == self.shown_offset)
            .or_else(|| self.frames.last())
    }

    fn summary(&self) -> Element<'_, Message> {
        let Some(place) = &self.place else {
            return widget::text(fl!("app-title")).into();
        };
        let condition = self
            .forecast
            .as_ref()
            .map(|forecast| describe(forecast.current.weather_code).label.to_string())
            .unwrap_or_default();
        let temp = self
            .panel_temperature()
            .map(|value| format!("{value}{}", self.unit_suffix()))
            .unwrap_or_default();
        widget::Column::new()
            .push(widget::text(format!("{}, {}", place.city, place.state)))
            .push(widget::text(format!("{temp}  {condition}")))
            .into()
    }

    fn map_block(&self) -> Element<'_, Message> {
        let map: Element<'_, Message> = if let Some(frame) = self.current_frame() {
            widget::image(frame.handle.clone())
                .width(Length::Fill)
                .height(Length::Fixed(240.0))
                .into()
        } else if !self.map_loading && !is_us_zip(&self.config.zip) {
            self.map_placeholder(fl!("enter-zip"))
        } else {
            self.map_placeholder(fl!("loading-radar"))
        };
        let label = self
            .current_frame()
            .map(|frame| frame_label(frame.offset_minutes))
            .unwrap_or_else(|| fl!("loading-radar"));
        let play = if self.playing {
            fl!("pause")
        } else {
            fl!("play")
        };
        let controls = widget::Row::new()
            .spacing(8)
            .align_y(Alignment::Center)
            .push(widget::text(label).width(Length::Fill))
            .push(widget::button::text(play).on_press(Message::TogglePlay));
        let mut column = widget::Column::new().spacing(6).push(map).push(controls);
        if self.radar_tiles == 0 && self.current_frame().is_some() {
            column = column.push(widget::text(fl!("radar-missing")).size(12));
        }
        if let Some(error) = &self.map_error {
            column = column.push(widget::text(error.clone()).size(12));
        }
        column
            .push(widget::text(self.basemap_caption()).size(12))
            .into()
    }

    fn map_placeholder(&self, label: String) -> Element<'_, Message> {
        widget::container(widget::text(label))
            .width(Length::Fill)
            .height(Length::Fixed(240.0))
            .into()
    }

    fn details(&self) -> Element<'_, Message> {
        let Some(forecast) = &self.forecast else {
            return widget::text("").into();
        };
        let feels = display_temperature(forecast.current.apparent_c, self.config.fahrenheit);
        let wind = if self.config.fahrenheit {
            format!("{} mph", display_wind_mph(forecast.current.wind_kmh))
        } else {
            format!(
                "{} km/h",
                display_temperature(forecast.current.wind_kmh, false)
            )
        };
        let high = self
            .forecast
            .as_ref()
            .and_then(|forecast| forecast.daily.first())
            .map(|day| {
                let high = display_temperature(day.temp_max_c, self.config.fahrenheit);
                let low = display_temperature(day.temp_min_c, self.config.fahrenheit);
                format!("{high}° / {low}°")
            })
            .unwrap_or_default();
        let as_of = format!(
            "ZIP {}, as of {} local time",
            self.config.zip,
            format_as_of(&forecast.current.time)
        );
        widget::Column::new()
            .spacing(2)
            .push(widget::text(as_of).size(12))
            .push(widget::text(format!(
                "{} {}{}",
                fl!("feels-like"),
                feels,
                self.unit_suffix()
            )))
            .push(widget::text(format!("{} {high}", fl!("high-low"))))
            .push(widget::text(format!("{} {wind}", fl!("wind"))))
            .push(widget::text(format!(
                "{} {}%",
                fl!("humidity"),
                forecast.current.humidity
            )))
            .into()
    }

    fn alert_block(&self) -> Element<'_, Message> {
        if self.alerts.is_empty() {
            return widget::text("").into();
        }
        let mut column = widget::Column::new()
            .spacing(4)
            .push(widget::text(fl!("alerts")));
        for alert in self.alerts.iter().take(3) {
            column = column.push(widget::text(alert.event.clone()));
            if !alert.headline.is_empty() {
                column = column.push(widget::text(alert.headline.clone()).size(12));
            }
        }
        column.into()
    }

    fn settings_block(&self) -> Element<'_, Message> {
        let zip = widget::text_input(fl!("zip-placeholder"), &self.zip_draft)
            .on_input(Message::ZipDraft)
            .on_submit(|_| Message::SaveZip);
        let key = widget::text_input(fl!("maptiler-placeholder"), &self.key_draft)
            .on_input(Message::KeyDraft)
            .on_submit(|_| Message::SaveKey);
        let units = widget::Row::new()
            .spacing(8)
            .push(widget::button::text(fl!("fahrenheit")).on_press(Message::SetFahrenheit(true)))
            .push(widget::button::text(fl!("celsius")).on_press(Message::SetFahrenheit(false)));
        widget::Column::new()
            .spacing(8)
            .push(
                widget::Row::new()
                    .spacing(8)
                    .push(zip)
                    .push(widget::button::text(fl!("save-zip")).on_press(Message::SaveZip)),
            )
            .push(widget::text(fl!("units")).size(12))
            .push(units)
            .push(widget::text(fl!("maptiler-key")).size(12))
            .push(key)
            .push(widget::settings::item(
                fl!("osm-basemap"),
                widget::toggler(self.config.allow_osm_basemap).on_toggle(Message::ToggleOsm),
            ))
            .into()
    }

    fn panel_icon(&self) -> &'static str {
        self.forecast
            .as_ref()
            .map(|forecast| symbolic_icon(describe(forecast.current.weather_code).icon))
            .unwrap_or("weather-clear-symbolic")
    }

    fn panel_temperature(&self) -> Option<String> {
        let forecast = self.forecast.as_ref()?;
        let value = display_temperature(forecast.current.temperature_c, self.config.fahrenheit);
        Some(format!("{value}°"))
    }

    fn unit_suffix(&self) -> &'static str {
        if self.config.fahrenheit { "F" } else { "C" }
    }

    fn tooltip(&self) -> String {
        match (&self.place, self.panel_temperature()) {
            (Some(place), Some(temp)) => format!("{} {temp}", place.city),
            _ => fl!("tooltip-empty"),
        }
    }

    fn basemap_caption(&self) -> String {
        match basemap_kind(&self.config) {
            BasemapKind::MapTiler(_) => fl!("basemap-maptiler"),
            BasemapKind::Osm => fl!("basemap-osm"),
            BasemapKind::None => fl!("basemap-none"),
        }
    }
}

fn frame_label(offset: u32) -> String {
    if offset == 0 {
        fl!("latest-scan")
    } else {
        fl!("minutes-ago", minutes = i64::from(offset))
    }
}

fn basemap_kind(config: &Config) -> BasemapKind {
    if let Ok(value) = std::env::var("MAPTILER_API_KEY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return BasemapKind::MapTiler(trimmed.to_string());
        }
    }
    let trimmed = config.maptiler_api_key.trim();
    if !trimmed.is_empty() {
        return BasemapKind::MapTiler(trimmed.to_string());
    }
    if config.allow_osm_basemap {
        BasemapKind::Osm
    } else {
        BasemapKind::None
    }
}
