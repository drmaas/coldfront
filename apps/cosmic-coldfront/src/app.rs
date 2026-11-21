// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use cosmic::iced::keyboard::{self, Modifiers};
use cosmic::iced::mouse::Button as MouseButton;
use cosmic::iced::mouse::ScrollDelta;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{
    destroy_popup, get_popup, set_size as set_popup_size,
};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Event, Limits, Point, Size, Subscription, Task, event};
use cosmic::prelude::*;
use cosmic::widget;
use cosmic::widget::segmented_button;

use coldfront_core::{
    APPLET_ZOOM_MAX, APPLET_ZOOM_MIN, Alert, Forecast, HttpClient, Place, applet_radar_frames,
    describe, display_icon, display_temperature, is_us_zip, symbolic_icon,
};

use crate::config::{self, Config};
use crate::fl;
use crate::mapload::{
    self, BasemapKind, LoadedFrame, MAP_VIEW_HEIGHT, MAP_VIEW_WIDTH, TileCache, WeatherBundle,
    viewport_for,
};
use crate::view;

pub const APP_ID: &str = "io.github.drmaas.CosmicColdfront";

const POPUP_WIDTH: f32 = 480.0;
const POPUP_HEIGHT: f32 = 440.0;
const RADAR_EXPAND_FALLBACK_WIDTH: f32 = 960.0;
const RADAR_EXPAND_FALLBACK_HEIGHT: f32 = 820.0;
const RADAR_EXPAND_MARGIN: f32 = 64.0;
const WEATHER_REFRESH: Duration = Duration::from_secs(30 * 60);
/// Debounce before fetching newly visible edge tiles. Short so pan feels like
/// the web canvas (tiles stream in while the camera moves).
const MAP_SETTLE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PopupTab {
    #[default]
    Current,
    Radar,
    Hourly,
    Forecast,
    Alerts,
    Settings,
}

pub struct AppModel {
    pub(crate) core: cosmic::Core,
    pub(crate) popup: Option<Id>,
    pub(crate) config: Config,
    pub(crate) zip_draft: String,
    pub(crate) key_draft: String,
    generation: u64,
    map_generation: u64,
    settle_token: u64,
    pub(crate) place: Option<Place>,
    pub(crate) forecast: Option<Forecast>,
    pub(crate) alerts: Vec<Alert>,
    pub(crate) weather_loading: bool,
    pub(crate) refresh_failed: bool,
    pub(crate) map_error: Option<String>,
    pub(crate) selected_forecast_day: Option<usize>,
    pub(crate) active_tab: PopupTab,
    pub(crate) tab_model: segmented_button::SingleSelectModel,
    pub(crate) unit_model: segmented_button::SingleSelectModel,
    // Radar
    tile_cache: TileCache,
    pub(crate) map_handle: Option<cosmic::iced::widget::image::Handle>,
    pub(crate) map_loading: bool,
    pub(crate) radar_tiles: usize,
    pub(crate) shown_offset: u32,
    pub(crate) playing: bool,
    pub(crate) zoom: u32,
    pub(crate) pan_x: f64,
    pub(crate) pan_y: f64,
    pending_offsets: Vec<u32>,
    frames_ready: Vec<u32>,
    drag: Option<DragState>,
    pointer: Option<Point>,
    ctrl_held: bool,
    radar_opened: bool,
    pub(crate) radar_expanded: bool,
    expanded_size: Option<Size>,
    startup_fetch_done: bool,
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    start: Point,
    pan_x: f64,
    pan_y: f64,
}

#[derive(Clone, Debug)]
pub struct WeatherLoaded {
    generation: u64,
    bundle: WeatherBundle,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Surface(cosmic::surface::Action<Message>),
    UpdateConfig(Config),
    StartupFetch,
    RefreshWeather,
    ZipDraft(String),
    SaveZip,
    KeyDraft(String),
    SaveKey,
    ToggleOsm(bool),
    SelectTab(PopupTab),
    TabActivated(segmented_button::Entity),
    UnitActivated(segmented_button::Entity),
    TogglePlay,
    FrameSlider(u32),
    AdvanceRadar,
    ZoomIn,
    ZoomOut,
    Recenter,
    ToggleRadarExpand,
    RadarExpandWith(Option<Size>),
    MapPress,
    MapRelease,
    MapMove(Point),
    MapScroll(ScrollDelta),
    MapSettle(u64),
    Modifiers(Modifiers),
    WeatherReady(Box<WeatherLoaded>),
    WeatherFailed {
        generation: u64,
        message: String,
    },
    MapReady {
        generation: u64,
        zoom: u32,
        pan_x: f64,
        pan_y: f64,
        frame: Box<LoadedFrame>,
        pending: Vec<u32>,
        cache: Box<TileCache>,
    },
    OlderFrame {
        generation: u64,
        zoom: u32,
        pan_x: f64,
        pan_y: f64,
        offset_minutes: u32,
        frame: Box<LoadedFrame>,
        cache: Box<TileCache>,
    },
    FrameSkipped {
        generation: u64,
        offset_minutes: u32,
    },
    MapFailed {
        generation: u64,
        message: String,
    },
    SelectForecastDay(usize),
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
        mut core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Frosted applet colors are translucent. An empty blur set keeps the
        // opaque palette so the popup does not show the panel through it.
        core.set_auto_blur(Default::default());
        let config = config::load();
        let active_tab = PopupTab::Current;
        let app = Self {
            core,
            popup: None,
            zip_draft: config.zip.clone(),
            key_draft: config.maptiler_api_key.clone(),
            tab_model: build_tab_model(active_tab),
            unit_model: build_unit_model(config.fahrenheit),
            config,
            generation: 0,
            map_generation: 0,
            settle_token: 0,
            place: None,
            forecast: None,
            alerts: Vec::new(),
            weather_loading: false,
            refresh_failed: false,
            map_error: None,
            selected_forecast_day: Some(0),
            active_tab,
            tile_cache: TileCache::default(),
            map_handle: None,
            map_loading: false,
            radar_tiles: 0,
            shown_offset: 0,
            playing: true,
            zoom: 7,
            pan_x: 0.0,
            pan_y: 0.0,
            pending_offsets: Vec::new(),
            frames_ready: Vec::new(),
            drag: None,
            pointer: None,
            ctrl_held: false,
            radar_opened: false,
            radar_expanded: false,
            expanded_size: None,
            startup_fetch_done: false,
        };
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        view::panel(self)
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if self.popup != Some(id) {
            return widget::text("").into();
        }
        view::popup(self)
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = vec![
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
            event::listen_with(|event, _status, _id| match event {
                Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                    Some(Message::Modifiers(modifiers))
                }
                Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(MouseButton::Left)) => {
                    Some(Message::MapRelease)
                }
                _ => None,
            }),
        ];
        if !self.startup_fetch_done {
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_millis(50)).map(|_| Message::StartupFetch),
            );
        } else if is_us_zip(&self.config.zip) {
            subscriptions
                .push(cosmic::iced::time::every(WEATHER_REFRESH).map(|_| Message::RefreshWeather));
        }
        if self.popup.is_some()
            && self.active_tab == PopupTab::Radar
            && self.playing
            && self.frames_ready.len() > 1
        {
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_millis(800))
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
                    self.drag = None;
                    self.radar_expanded = false;
                    self.expanded_size = None;
                }
            }
            Message::Surface(action) => return Task::done(cosmic::action::surface(action)),
            Message::UpdateConfig(config) => {
                let zip_changed = config.zip != self.config.zip;
                let key_changed = config.maptiler_api_key != self.config.maptiler_api_key;
                let osm_changed = config.allow_osm_basemap != self.config.allow_osm_basemap;
                self.config = config;
                self.zip_draft.clone_from(&self.config.zip);
                self.key_draft.clone_from(&self.config.maptiler_api_key);
                self.unit_model = build_unit_model(self.config.fahrenheit);
                if zip_changed {
                    return self.start_weather_load(true);
                }
                if (key_changed || osm_changed) && self.radar_opened {
                    return self.reload_map(true);
                }
            }
            Message::StartupFetch => {
                if self.startup_fetch_done {
                    return Task::none();
                }
                self.startup_fetch_done = true;
                if is_us_zip(&self.config.zip) {
                    return self.start_weather_load(false);
                }
            }
            Message::RefreshWeather => {
                if is_us_zip(&self.config.zip) {
                    return self.start_weather_load(false);
                }
            }
            Message::ZipDraft(value) => self.zip_draft = value,
            Message::SaveZip => {
                self.config.zip = self.zip_draft.trim().to_string();
                config::save(&self.config);
                return self.start_weather_load(true);
            }
            Message::KeyDraft(value) => self.key_draft = value,
            Message::SaveKey => {
                self.config.maptiler_api_key = self.key_draft.trim().to_string();
                config::save(&self.config);
                if self.radar_opened {
                    return self.reload_map(true);
                }
            }
            Message::ToggleOsm(enabled) => {
                self.config.allow_osm_basemap = enabled;
                config::save(&self.config);
                if self.radar_opened {
                    return self.reload_map(true);
                }
            }
            Message::SelectTab(tab) => return self.select_tab(tab),
            Message::TabActivated(entity) => {
                if let Some(&tab) = self.tab_model.data::<PopupTab>(entity) {
                    return self.select_tab(tab);
                }
            }
            Message::UnitActivated(entity) => {
                if let Some(&fahrenheit) = self.unit_model.data::<bool>(entity) {
                    self.config.fahrenheit = fahrenheit;
                    config::save(&self.config);
                    self.unit_model = build_unit_model(fahrenheit);
                }
            }
            Message::TogglePlay => self.playing = !self.playing,
            Message::FrameSlider(index) => {
                self.playing = false;
                if let Some(offset) = frame_offset_at(index as usize) {
                    self.shown_offset = offset;
                    self.recompose_current();
                }
            }
            Message::AdvanceRadar => self.advance_radar(),
            Message::ZoomIn => return self.set_zoom(self.zoom.saturating_add(1)),
            Message::ZoomOut => return self.set_zoom(self.zoom.saturating_sub(1)),
            Message::Recenter => {
                self.pan_x = 0.0;
                self.pan_y = 0.0;
                self.recompose_current();
                return self.schedule_settle();
            }
            Message::ToggleRadarExpand => {
                if self.radar_expanded {
                    return self.collapse_radar();
                }
                let Some(id) = self.core.main_window_id().or(self.popup) else {
                    return Task::none();
                };
                return window::monitor_size(id)
                    .map(|size| cosmic::Action::App(Message::RadarExpandWith(size)));
            }
            Message::RadarExpandWith(size) => return self.expand_radar(size),
            Message::MapPress => {
                if let Some(point) = self.pointer {
                    self.drag = Some(DragState {
                        start: point,
                        pan_x: self.pan_x,
                        pan_y: self.pan_y,
                    });
                }
            }
            Message::MapRelease => {
                if self.drag.take().is_some() {
                    return self.schedule_settle();
                }
            }
            Message::MapMove(point) => {
                self.pointer = Some(point);
                if let Some(drag) = self.drag {
                    self.pan_x = drag.pan_x + f64::from(point.x - drag.start.x);
                    self.pan_y = drag.pan_y + f64::from(point.y - drag.start.y);
                    self.recompose_current();
                }
            }
            Message::MapScroll(delta) => return self.handle_scroll(delta),
            Message::MapSettle(token) => {
                if token == self.settle_token {
                    return self.refresh_visible_map();
                }
            }
            Message::Modifiers(modifiers) => self.ctrl_held = modifiers.control(),
            Message::WeatherReady(loaded) => {
                if loaded.generation != self.generation {
                    return Task::none();
                }
                let place_changed = self.place.as_ref().is_none_or(|place| {
                    (place.latitude - loaded.bundle.place.latitude).abs() > f64::EPSILON
                        || (place.longitude - loaded.bundle.place.longitude).abs() > f64::EPSILON
                });
                self.place = Some(loaded.bundle.place);
                self.forecast = Some(loaded.bundle.forecast);
                self.alerts = loaded.bundle.alerts;
                self.weather_loading = false;
                self.refresh_failed = false;
                self.map_error = None;
                self.selected_forecast_day = Some(0);
                if self.active_tab == PopupTab::Radar || self.radar_opened {
                    let clear_cache = place_changed || self.map_handle.is_none();
                    return self.reload_map(clear_cache);
                }
            }
            Message::WeatherFailed {
                generation,
                message,
            } => {
                if generation != self.generation {
                    return Task::none();
                }
                self.weather_loading = false;
                if self.forecast.is_some() {
                    self.refresh_failed = true;
                } else {
                    self.map_error = Some(message);
                }
            }
            Message::MapReady {
                generation,
                zoom,
                pan_x,
                pan_y,
                frame,
                pending,
                cache,
            } => {
                self.tile_cache.absorb(&cache);
                if generation != self.map_generation {
                    return Task::none();
                }
                self.map_loading = false;
                self.pending_offsets = pending;
                self.apply_frame(*frame, zoom, pan_x, pan_y);
                return self.load_next_frame();
            }
            Message::OlderFrame {
                generation,
                zoom,
                pan_x,
                pan_y,
                offset_minutes,
                frame,
                cache,
            } => {
                self.tile_cache.absorb(&cache);
                if generation != self.map_generation {
                    return Task::none();
                }
                self.finish_pending(offset_minutes);
                self.apply_frame(*frame, zoom, pan_x, pan_y);
                return self.load_next_frame();
            }
            Message::FrameSkipped {
                generation,
                offset_minutes,
            } => {
                if generation == self.map_generation {
                    self.finish_pending(offset_minutes);
                    return self.load_next_frame();
                }
            }
            Message::MapFailed {
                generation,
                message,
            } => {
                if generation == self.map_generation {
                    self.map_loading = false;
                    // Keep a painted map after a soft edge-tile refresh fails.
                    if self.map_handle.is_none() {
                        self.map_error = Some(message);
                    }
                }
            }
            Message::SelectForecastDay(index) => {
                if self
                    .forecast
                    .as_ref()
                    .is_some_and(|forecast| index < forecast.daily.len())
                {
                    self.selected_forecast_day = Some(index);
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
            self.radar_expanded = false;
            self.expanded_size = None;
            return destroy_popup(id);
        }
        self.radar_expanded = false;
        self.expanded_size = None;
        let Some(parent) = self.core.main_window_id() else {
            return Task::none();
        };
        let id = Id::unique();
        self.popup = Some(id);
        let mut settings = self
            .core
            .applet
            .get_popup_settings(parent, id, None, None, None);
        settings.positioner.size_limits = self.popup_limits();
        let open = get_popup(settings);
        if self.forecast.is_none() && is_us_zip(&self.config.zip) && !self.weather_loading {
            Task::batch([open, self.start_weather_load(false)])
        } else {
            open
        }
    }

    fn select_tab(&mut self, tab: PopupTab) -> Task<cosmic::Action<Message>> {
        let leaving_expanded_radar =
            self.radar_expanded && self.active_tab == PopupTab::Radar && tab != PopupTab::Radar;
        self.active_tab = tab;
        self.tab_model = if matches!(tab, PopupTab::Settings | PopupTab::Alerts) {
            build_tab_model_none()
        } else {
            build_tab_model(tab)
        };
        let mut tasks = Vec::new();
        if leaving_expanded_radar {
            tasks.push(self.collapse_radar());
        }
        if tab == PopupTab::Radar {
            self.radar_opened = true;
            if self.place.is_some() && (self.map_handle.is_none() || self.frames_ready.is_empty()) {
                tasks.push(self.reload_map(true));
            }
        }
        Task::batch(tasks)
    }

    fn expand_radar(&mut self, monitor: Option<Size>) -> Task<cosmic::Action<Message>> {
        let size = match monitor {
            Some(size) => Size::new(
                (size.width - RADAR_EXPAND_MARGIN).max(POPUP_WIDTH),
                (size.height - RADAR_EXPAND_MARGIN).max(POPUP_HEIGHT),
            ),
            None => Size::new(RADAR_EXPAND_FALLBACK_WIDTH, RADAR_EXPAND_FALLBACK_HEIGHT),
        };
        self.radar_expanded = true;
        self.expanded_size = Some(size);
        Task::batch([self.resize_popup(), self.apply_radar_size_change()])
    }

    fn collapse_radar(&mut self) -> Task<cosmic::Action<Message>> {
        self.radar_expanded = false;
        self.expanded_size = None;
        Task::batch([self.resize_popup(), self.apply_radar_size_change()])
    }

    /// `window::resize` does not reach an xdg popup. Size goes through the popup positioner.
    fn resize_popup(&self) -> Task<cosmic::Action<Message>> {
        let Some(id) = self.popup else {
            return Task::none();
        };
        let size = self.popup_size();
        set_popup_size(
            id,
            size.width.round().max(1.0) as u32,
            size.height.round().max(1.0) as u32,
        )
    }

    fn apply_radar_size_change(&mut self) -> Task<cosmic::Action<Message>> {
        self.recompose_current();
        self.refresh_visible_map()
    }

    fn popup_limits(&self) -> Limits {
        let size = self.popup_size();
        Limits::NONE
            .min_width(size.width)
            .max_width(size.width)
            .min_height(size.height)
            .max_height(size.height)
    }

    fn popup_size(&self) -> Size {
        if self.radar_expanded {
            self.expanded_size.unwrap_or(Size::new(
                RADAR_EXPAND_FALLBACK_WIDTH,
                RADAR_EXPAND_FALLBACK_HEIGHT,
            ))
        } else {
            Size::new(POPUP_WIDTH, POPUP_HEIGHT)
        }
    }

    pub(crate) fn popup_container_limits(&self) -> Limits {
        self.popup_limits()
    }

    pub(crate) fn popup_width(&self) -> f32 {
        self.popup_size().width
    }

    pub(crate) fn popup_height(&self) -> f32 {
        self.popup_size().height
    }

    fn start_weather_load(&mut self, zip_changed: bool) -> Task<cosmic::Action<Message>> {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.weather_loading = true;
        self.refresh_failed = false;
        self.map_error = None;
        if zip_changed {
            self.reset_map_state();
            self.place = None;
            self.forecast = None;
            self.alerts.clear();
        }
        let zip = self.config.zip.clone();
        if !is_us_zip(&zip) {
            self.weather_loading = false;
            self.place = None;
            self.forecast = None;
            self.alerts.clear();
            self.map_error = Some(coldfront_core::Error::InvalidZip.to_string());
            return Task::none();
        }
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
            match mapload::load_weather(&client, &zip).await {
                Ok(bundle) => Message::WeatherReady(Box::new(WeatherLoaded { generation, bundle })),
                Err(error) => Message::WeatherFailed {
                    generation,
                    message: error.to_string(),
                },
            }
        })
    }

    fn reset_map_state(&mut self) {
        self.map_generation = self.map_generation.wrapping_add(1);
        self.tile_cache.clear();
        self.map_handle = None;
        self.frames_ready.clear();
        self.pending_offsets.clear();
        self.radar_tiles = 0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.zoom = 7;
        self.shown_offset = 0;
        self.radar_opened = false;
        self.map_loading = false;
    }

    fn reload_map(&mut self, clear_cache: bool) -> Task<cosmic::Action<Message>> {
        let Some(place) = self.place.clone() else {
            return Task::none();
        };
        self.map_generation = self.map_generation.wrapping_add(1);
        let generation = self.map_generation;
        if clear_cache {
            self.tile_cache.clear();
            self.frames_ready.clear();
            self.map_handle = None;
        }
        // Keep the last composite on screen while edge tiles refill after pan.
        self.map_loading = self.map_handle.is_none();
        self.map_error = None;
        let kind = basemap_kind(&self.config);
        let zoom = self.zoom;
        let pan_x = self.pan_x;
        let pan_y = self.pan_y;
        let (view_width, view_height) = self.map_size();
        let shown_offset = self.shown_offset;
        let mut cache = self.tile_cache.clone();
        cosmic::task::future(async move {
            let client = match HttpClient::new() {
                Ok(client) => client,
                Err(error) => {
                    return Message::MapFailed {
                        generation,
                        message: error.to_string(),
                    };
                }
            };
            let frames = applet_radar_frames();
            let latest = frames.last().map(|frame| frame.offset_minutes).unwrap_or(0);
            let first_offset = if frames
                .iter()
                .any(|frame| frame.offset_minutes == shown_offset)
            {
                shown_offset
            } else {
                latest
            };
            let request = mapload::MapRequest {
                latitude: place.latitude,
                longitude: place.longitude,
                zoom,
                pan_x,
                pan_y,
                view_width,
                view_height,
                offset_minutes: first_offset,
            };
            match mapload::ensure_map_frame(&client, &mut cache, &kind, &request).await {
                Ok(frame) => {
                    let mut pending: Vec<u32> = frames
                        .iter()
                        .rev()
                        .map(|item| item.offset_minutes)
                        .filter(|offset| *offset != first_offset)
                        .collect();
                    // Prefer the latest scan next when we loaded some other frame first.
                    if first_offset != latest
                        && let Some(index) = pending.iter().position(|offset| *offset == latest)
                    {
                        let value = pending.remove(index);
                        pending.push(value);
                    }
                    Message::MapReady {
                        generation,
                        zoom,
                        pan_x,
                        pan_y,
                        frame: Box::new(frame),
                        pending,
                        cache: Box::new(cache),
                    }
                }
                Err(error) => Message::MapFailed {
                    generation,
                    message: error.to_string(),
                },
            }
        })
    }

    fn load_next_frame(&mut self) -> Task<cosmic::Action<Message>> {
        let Some(&offset) = self.pending_offsets.last() else {
            return Task::none();
        };
        let Some(place) = self.place.clone() else {
            return Task::none();
        };
        let generation = self.map_generation;
        let kind = basemap_kind(&self.config);
        let zoom = self.zoom;
        let pan_x = self.pan_x;
        let pan_y = self.pan_y;
        let (view_width, view_height) = self.map_size();
        let mut cache = self.tile_cache.clone();
        cosmic::task::future(async move {
            let client = match HttpClient::new() {
                Ok(client) => client,
                Err(_) => {
                    return Message::FrameSkipped {
                        generation,
                        offset_minutes: offset,
                    };
                }
            };
            let request = mapload::MapRequest {
                latitude: place.latitude,
                longitude: place.longitude,
                zoom,
                pan_x,
                pan_y,
                view_width,
                view_height,
                offset_minutes: offset,
            };
            match mapload::ensure_map_frame(&client, &mut cache, &kind, &request).await {
                Ok(frame) => Message::OlderFrame {
                    generation,
                    zoom,
                    pan_x,
                    pan_y,
                    offset_minutes: offset,
                    frame: Box::new(frame),
                    cache: Box::new(cache),
                },
                Err(_) => Message::FrameSkipped {
                    generation,
                    offset_minutes: offset,
                },
            }
        })
    }

    fn finish_pending(&mut self, offset_minutes: u32) {
        if self.pending_offsets.last() == Some(&offset_minutes) {
            self.pending_offsets.pop();
        } else {
            self.pending_offsets
                .retain(|offset| *offset != offset_minutes);
        }
    }

    fn camera_matches(&self, zoom: u32, pan_x: f64, pan_y: f64) -> bool {
        zoom == self.zoom && (pan_x - self.pan_x).abs() < 0.5 && (pan_y - self.pan_y).abs() < 0.5
    }

    fn apply_frame(&mut self, frame: LoadedFrame, zoom: u32, pan_x: f64, pan_y: f64) {
        if !self.frames_ready.contains(&frame.offset_minutes) {
            self.frames_ready.push(frame.offset_minutes);
            self.frames_ready.sort_unstable();
        }
        if self.frames_ready.len() == 1 {
            self.shown_offset = frame.offset_minutes;
        }
        if !self.camera_matches(zoom, pan_x, pan_y) {
            self.recompose_current();
            return;
        }
        if self.shown_offset == frame.offset_minutes {
            self.radar_tiles = frame.radar_tiles;
            self.map_handle = Some(cosmic::iced::widget::image::Handle::from_rgba(
                frame.width,
                frame.height,
                frame.pixels,
            ));
        } else {
            self.recompose_current();
        }
    }

    fn recompose_current(&mut self) {
        let Some(place) = &self.place else {
            return;
        };
        let (view_width, view_height) = self.map_size();
        let viewport = viewport_for(
            place.latitude,
            place.longitude,
            self.zoom,
            self.pan_x,
            self.pan_y,
            view_width,
            view_height,
        );
        let kind = basemap_kind(&self.config);
        let Ok(frame) =
            mapload::compose_cached(&self.tile_cache, &kind, &viewport, self.shown_offset)
        else {
            return;
        };
        self.radar_tiles = frame.radar_tiles;
        self.map_handle = Some(cosmic::iced::widget::image::Handle::from_rgba(
            frame.width,
            frame.height,
            frame.pixels,
        ));
    }

    /// Fetch tiles for the current camera without blanking the shown map.
    ///
    /// First load still goes through `reload_map`. Later pan/zoom settles fill
    /// edge tiles for the shown frame and every frame already ready, then
    /// recompose so basemap and radar stay locked.
    fn refresh_visible_map(&mut self) -> Task<cosmic::Action<Message>> {
        if self.map_handle.is_none() || self.frames_ready.is_empty() {
            return self.reload_map(false);
        }
        let Some(place) = self.place.clone() else {
            return Task::none();
        };
        self.map_generation = self.map_generation.wrapping_add(1);
        let generation = self.map_generation;
        self.map_error = None;
        let kind = basemap_kind(&self.config);
        let zoom = self.zoom;
        let pan_x = self.pan_x;
        let pan_y = self.pan_y;
        let (view_width, view_height) = self.map_size();
        let shown_offset = self.shown_offset;
        let extra_offsets: Vec<u32> = self
            .frames_ready
            .iter()
            .copied()
            .filter(|offset| *offset != shown_offset)
            .collect();
        // Same order as reload_map: `.last()` loads next, so reverse chrono.
        let pending: Vec<u32> = applet_radar_frames()
            .into_iter()
            .rev()
            .map(|frame| frame.offset_minutes)
            .filter(|offset| !self.frames_ready.contains(offset) && *offset != shown_offset)
            .collect();
        let mut cache = self.tile_cache.clone();
        cosmic::task::future(async move {
            let client = match HttpClient::new() {
                Ok(client) => client,
                Err(error) => {
                    return Message::MapFailed {
                        generation,
                        message: error.to_string(),
                    };
                }
            };
            let request = mapload::MapRequest {
                latitude: place.latitude,
                longitude: place.longitude,
                zoom,
                pan_x,
                pan_y,
                view_width,
                view_height,
                offset_minutes: shown_offset,
            };
            match mapload::ensure_map_frames(&client, &mut cache, &kind, &request, &extra_offsets)
                .await
            {
                Ok(frame) => Message::MapReady {
                    generation,
                    zoom,
                    pan_x,
                    pan_y,
                    frame: Box::new(frame),
                    pending,
                    cache: Box::new(cache),
                },
                Err(error) => Message::MapFailed {
                    generation,
                    message: error.to_string(),
                },
            }
        })
    }

    fn advance_radar(&mut self) {
        let sequence: Vec<u32> = applet_radar_frames()
            .into_iter()
            .map(|frame| frame.offset_minutes)
            .filter(|offset| self.frames_ready.contains(offset))
            .collect();
        if sequence.len() < 2 {
            return;
        }
        let index = sequence
            .iter()
            .position(|offset| *offset == self.shown_offset)
            .unwrap_or(0);
        let next = (index + 1) % sequence.len();
        self.shown_offset = sequence[next];
        self.recompose_current();
    }

    fn set_zoom(&mut self, zoom: u32) -> Task<cosmic::Action<Message>> {
        let zoom = zoom.clamp(APPLET_ZOOM_MIN, APPLET_ZOOM_MAX);
        if zoom == self.zoom {
            return Task::none();
        }
        let scale = 2f64.powi(zoom as i32 - self.zoom as i32);
        self.pan_x *= scale;
        self.pan_y *= scale;
        self.zoom = zoom;
        self.recompose_current();
        self.schedule_settle()
    }

    fn handle_scroll(&mut self, delta: ScrollDelta) -> Task<cosmic::Action<Message>> {
        match delta {
            ScrollDelta::Lines { y, .. } => {
                if y > 0.0 {
                    self.set_zoom(self.zoom.saturating_add(1))
                } else if y < 0.0 {
                    self.set_zoom(self.zoom.saturating_sub(1))
                } else {
                    Task::none()
                }
            }
            ScrollDelta::Pixels { x, y } => {
                if self.ctrl_held {
                    if y > 0.0 {
                        self.set_zoom(self.zoom.saturating_add(1))
                    } else if y < 0.0 {
                        self.set_zoom(self.zoom.saturating_sub(1))
                    } else {
                        Task::none()
                    }
                } else {
                    self.pan_x += f64::from(x);
                    self.pan_y += f64::from(y);
                    self.recompose_current();
                    self.schedule_settle()
                }
            }
        }
    }

    fn schedule_settle(&mut self) -> Task<cosmic::Action<Message>> {
        self.settle_token = self.settle_token.wrapping_add(1);
        let token = self.settle_token;
        cosmic::task::future(async move {
            tokio::time::sleep(MAP_SETTLE).await;
            Message::MapSettle(token)
        })
    }

    pub(crate) fn panel_icon(&self) -> &'static str {
        self.forecast
            .as_ref()
            .map(|forecast| {
                let info = describe(forecast.current.weather_code);
                symbolic_icon(display_icon(info.icon, forecast.current.is_day))
            })
            .unwrap_or("weather-clear-symbolic")
    }

    pub(crate) fn panel_temperature(&self) -> Option<String> {
        let forecast = self.forecast.as_ref()?;
        Some(format_panel_temperature(
            forecast.current.temperature_c,
            self.config.fahrenheit,
        ))
    }

    pub(crate) fn unit_suffix(&self) -> &'static str {
        if self.config.fahrenheit { "F" } else { "C" }
    }

    pub(crate) fn tooltip(&self) -> String {
        match (&self.place, self.panel_temperature()) {
            (Some(place), Some(temp)) => format!("{} {temp}", place.city),
            _ => fl!("tooltip-empty"),
        }
    }

    pub(crate) fn basemap_caption(&self) -> String {
        match basemap_kind(&self.config) {
            BasemapKind::MapTiler(_) => fl!("basemap-maptiler"),
            BasemapKind::Osm => fl!("basemap-osm"),
            BasemapKind::None => fl!("basemap-none"),
        }
    }

    pub(crate) fn frame_index(&self) -> u32 {
        applet_radar_frames()
            .iter()
            .position(|frame| frame.offset_minutes == self.shown_offset)
            .unwrap_or(0) as u32
    }

    pub(crate) fn frame_count(&self) -> u32 {
        applet_radar_frames().len().saturating_sub(1) as u32
    }

    pub(crate) fn current_frame_label(&self) -> String {
        frame_label(self.shown_offset)
    }

    pub(crate) fn map_size(&self) -> (u32, u32) {
        if self.radar_expanded {
            let size = self.popup_size();
            (
                (size.width - 48.0).max(400.0) as u32,
                (size.height - 220.0).max(240.0) as u32,
            )
        } else {
            (MAP_VIEW_WIDTH, MAP_VIEW_HEIGHT)
        }
    }

    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }
}

fn frame_label(offset: u32) -> String {
    if offset == 0 {
        fl!("latest-scan")
    } else {
        fl!("minutes-ago", minutes = i64::from(offset))
    }
}

fn frame_offset_at(index: usize) -> Option<u32> {
    applet_radar_frames()
        .get(index)
        .map(|frame| frame.offset_minutes)
}

fn build_tab_model(active: PopupTab) -> segmented_button::SingleSelectModel {
    let mut model = segmented_button::SingleSelectModel::default();
    for (tab, label) in [
        (PopupTab::Current, fl!("tab-current")),
        (PopupTab::Radar, fl!("tab-radar")),
        (PopupTab::Hourly, fl!("tab-hourly")),
        (PopupTab::Forecast, fl!("tab-forecast")),
    ] {
        let id = model.insert().text(label).data(tab).id();
        if active == tab {
            model.activate(id);
        }
    }
    model
}

pub(crate) fn format_panel_temperature(celsius: f64, fahrenheit: bool) -> String {
    let value = display_temperature(celsius, fahrenheit);
    let unit = if fahrenheit { "F" } else { "C" };
    format!("{value}°{unit}")
}

fn build_tab_model_none() -> segmented_button::SingleSelectModel {
    let mut model = segmented_button::SingleSelectModel::default();
    for (tab, label) in [
        (PopupTab::Current, fl!("tab-current")),
        (PopupTab::Radar, fl!("tab-radar")),
        (PopupTab::Hourly, fl!("tab-hourly")),
        (PopupTab::Forecast, fl!("tab-forecast")),
    ] {
        model.insert().text(label).data(tab);
    }
    model
}

fn build_unit_model(fahrenheit: bool) -> segmented_button::SingleSelectModel {
    let mut model = segmented_button::SingleSelectModel::default();
    let f_id = model.insert().text(fl!("fahrenheit")).data(true).id();
    let c_id = model.insert().text(fl!("celsius")).data(false).id();
    if fahrenheit {
        model.activate(f_id);
    } else {
        model.activate(c_id);
    }
    model
}

pub(crate) fn basemap_kind(config: &Config) -> BasemapKind {
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

#[cfg(test)]
mod tests {
    use super::format_panel_temperature;

    #[test]
    fn panel_temperature_includes_unit() {
        assert_eq!(format_panel_temperature(11.666_666_666_7, true), "53°F");
        assert_eq!(format_panel_temperature(12.4, false), "12°C");
        assert_eq!(format_panel_temperature(-1.5, false), "-2°C");
    }
}
