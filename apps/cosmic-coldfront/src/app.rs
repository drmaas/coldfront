// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

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
    Alert, EchoField, Forecast, GeocodeHit, GestureCamera, HttpClient, Place, RadarFrame,
    RadarLookback, RadarSpeed, Raster, ReliefCamera, SnapshotTransform,
    applet_radar_frames_lookback, applet_radar_interval_ms, build_echo_field, clamp_relief_pitch,
    default_relief_camera, describe, display_icon, display_temperature, echo_grid,
    format_radar_clock, geocode_query_allowed, is_us_zip, is_zip_lookup, place_from_geocode_hit,
    render_relief, sample_echoes, snapshot_transform, symbolic_icon, zoom_toward,
};

const GEOCODE_SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

use crate::config::{self, Config};
use crate::fl;
use crate::mapload::{
    self, BasemapKind, LoadedFrame, MAP_VIEW_HEIGHT, MAP_VIEW_WIDTH, TileCache, WeatherBundle,
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
/// Longest gap between paints while the camera moves. Pointer motion slides the
/// last frame between these, so compose runs at most once per display frame.
const GESTURE_FRAME: Duration = Duration::from_millis(16);
/// Most pixels a gesture frame may compose. Larger views render smaller and the
/// canvas scales them up until settle, so a fullscreen pan paints inside a
/// frame instead of blocking the event loop on a resample.
const GESTURE_PIXEL_BUDGET: f64 = 500_000.0;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RadarMode {
    #[default]
    Flat,
    Relief,
}

pub struct AppModel {
    pub(crate) core: cosmic::Core,
    pub(crate) popup: Option<Id>,
    pub(crate) config: Config,
    pub(crate) zip_draft: String,
    pub(crate) geocode_hits: Vec<GeocodeHit>,
    geocode_hits_query: String,
    pub(crate) geocode_loading: bool,
    pub(crate) geocode_error: Option<String>,
    geocode_generation: u64,
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
    /// Playback frame to apply on the next settle when a gesture is in progress.
    deferred_shown_offset: Option<u32>,
    pub(crate) playing: bool,
    pub(crate) radar_speed: RadarSpeed,
    pub(crate) radar_lookback: RadarLookback,
    /// Frozen lookback list (instants match the tiles that were requested).
    radar_frames: Vec<RadarFrame>,
    pub(crate) zoom: f64,
    pub(crate) pan_x: f64,
    pub(crate) pan_y: f64,
    /// Camera of the last presented snapshot (for gesture blit).
    settled_zoom: f64,
    settled_pan_x: f64,
    settled_pan_y: f64,
    settled_width: u32,
    settled_height: u32,
    pixel_ratio: f64,
    gesturing: bool,
    /// When a gesture last painted the map from cache.
    gesture_paint_at: Option<Instant>,
    /// A coalesced gesture paint is waiting out the frame budget.
    gesture_paint_scheduled: bool,
    pending_offsets: Vec<u32>,
    frames_ready: Vec<u32>,
    lookback_batch_inflight: bool,
    drag: Option<DragState>,
    pointer: Option<Point>,
    map_hovered: bool,
    ctrl_held: bool,
    shift_held: bool,
    radar_opened: bool,
    pub(crate) radar_expanded: bool,
    expanded_size: Option<Size>,
    pub(crate) radar_mode: RadarMode,
    relief_camera: ReliefCamera,
    relief_ground: Option<Raster>,
    relief_field: Option<EchoField>,
    pub(crate) relief_empty: bool,
    startup_fetch_done: bool,
}

#[derive(Clone, Copy, Debug)]
enum DragState {
    Pan {
        start: Point,
        pan_x: f64,
        pan_y: f64,
    },
    Orbit {
        start: Point,
        yaw: f32,
        pitch: f32,
    },
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
    /// Dev-only: open Radar popup when `COLDFRONT_VERIFY` is set.
    VerifyOpenRadar,
    /// Dev-only: expand radar when `COLDFRONT_VERIFY_EXPAND` is set.
    VerifyExpandRadar,
    /// Dev-only: nudge pan for screenshot proof when `COLDFRONT_VERIFY_PAN` is set.
    VerifyNudgePan,
    RefreshWeather,
    ZipDraft(String),
    SaveZip,
    GeocodeSearch {
        generation: u64,
    },
    GeocodeReady {
        generation: u64,
        hits: Vec<GeocodeHit>,
    },
    GeocodeFailed {
        generation: u64,
        message: String,
    },
    PickGeocode(usize),
    KeyDraft(String),
    SaveKey,
    ToggleOsm(bool),
    SelectTab(PopupTab),
    TabActivated(segmented_button::Entity),
    UnitActivated(segmented_button::Entity),
    TogglePlay,
    CycleRadarSpeed,
    CycleRadarLookback,
    FrameSlider(u32),
    AdvanceRadar,
    ZoomIn,
    ZoomOut,
    Recenter,
    ToggleRadarExpand,
    RadarExpandWith(Option<Size>),
    ToggleRadarMode,
    MapPress,
    MapRightPress,
    MapRelease,
    MapLeave,
    MapMove(Point),
    MapScroll(ScrollDelta),
    MapSettle(u64),
    /// Deferred gesture paint, scheduled by [`AppModel::paint_gesture`].
    GestureCompose,
    PrefetchDone {
        generation: u64,
        cache: Box<TileCache>,
    },
    ScaleFactor(f64),
    Modifiers(Modifiers),
    WeatherReady(Box<WeatherLoaded>),
    WeatherFailed {
        generation: u64,
        message: String,
    },
    MapReady {
        generation: u64,
        zoom: f64,
        pan_x: f64,
        pan_y: f64,
        frame: Box<LoadedFrame>,
        pending: Vec<u32>,
        cache: Box<TileCache>,
    },
    OlderFrame {
        generation: u64,
        zoom: f64,
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
    LookbackBatch {
        generation: u64,
        newly_ready: Vec<u32>,
        attempted: Vec<u32>,
        cache: Box<TileCache>,
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
            zip_draft: location_draft(&config),
            geocode_hits: Vec::new(),
            geocode_hits_query: String::new(),
            geocode_loading: false,
            geocode_error: None,
            geocode_generation: 0,
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
            selected_forecast_day: None,
            active_tab,
            tile_cache: TileCache::default(),
            map_handle: None,
            map_loading: false,
            radar_tiles: 0,
            shown_offset: 0,
            deferred_shown_offset: None,
            playing: true,
            radar_speed: RadarSpeed::X1,
            radar_lookback: RadarLookback::Hours1,
            radar_frames: applet_radar_frames_lookback(RadarLookback::Hours1),
            zoom: 7.0,
            pan_x: 0.0,
            pan_y: 0.0,
            settled_zoom: 7.0,
            settled_pan_x: 0.0,
            settled_pan_y: 0.0,
            settled_width: MAP_VIEW_WIDTH,
            settled_height: MAP_VIEW_HEIGHT,
            pixel_ratio: 1.0,
            gesturing: false,
            gesture_paint_at: None,
            gesture_paint_scheduled: false,
            pending_offsets: Vec::new(),
            frames_ready: Vec::new(),
            lookback_batch_inflight: false,
            drag: None,
            pointer: None,
            map_hovered: false,
            ctrl_held: false,
            shift_held: false,
            radar_opened: false,
            radar_expanded: false,
            expanded_size: None,
            radar_mode: RadarMode::Flat,
            relief_camera: default_relief_camera(MAP_VIEW_WIDTH, MAP_VIEW_HEIGHT),
            relief_ground: None,
            relief_field: None,
            relief_empty: false,
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
                Event::Mouse(cosmic::iced::mouse::Event::ButtonPressed(MouseButton::Right)) => {
                    Some(Message::MapRightPress)
                }
                Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(MouseButton::Left))
                | Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(MouseButton::Right)) => {
                    Some(Message::MapRelease)
                }
                Event::Window(window::Event::Rescaled(scale)) => {
                    Some(Message::ScaleFactor(f64::from(scale)))
                }
                _ => None,
            }),
        ];
        if !self.startup_fetch_done {
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_millis(50)).map(|_| Message::StartupFetch),
            );
        } else if self.config.has_weather_location() {
            subscriptions
                .push(cosmic::iced::time::every(WEATHER_REFRESH).map(|_| Message::RefreshWeather));
        }
        if self.popup.is_some()
            && self.active_tab == PopupTab::Radar
            && self.playing
            && self.frames_ready.len() > 1
        {
            let interval_ms = u64::from(applet_radar_interval_ms(self.radar_speed));
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_millis(interval_ms))
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
                let location_changed = location_changed(&self.config, &config);
                let key_changed = config.maptiler_api_key != self.config.maptiler_api_key;
                let osm_changed = config.allow_osm_basemap != self.config.allow_osm_basemap;
                self.config = config;
                self.zip_draft = location_draft(&self.config);
                self.key_draft.clone_from(&self.config.maptiler_api_key);
                self.unit_model = build_unit_model(self.config.fahrenheit);
                if location_changed {
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
                let mut tasks = Vec::new();
                if self.config.has_weather_location() {
                    tasks.push(self.start_weather_load(false));
                }
                if std::env::var_os("COLDFRONT_VERIFY").is_some() {
                    tasks.push(cosmic::task::future(async {
                        tokio::time::sleep(Duration::from_millis(2000)).await;
                        Message::VerifyOpenRadar
                    }));
                }
                return Task::batch(tasks);
            }
            Message::VerifyOpenRadar => {
                let mut tasks = Vec::new();
                if self.popup.is_none() {
                    tasks.push(self.toggle_popup());
                }
                tasks.push(self.select_tab(PopupTab::Radar));
                if std::env::var_os("COLDFRONT_VERIFY_EXPAND").is_some() {
                    tasks.push(cosmic::task::future(async {
                        tokio::time::sleep(Duration::from_millis(5000)).await;
                        Message::VerifyExpandRadar
                    }));
                }
                if std::env::var_os("COLDFRONT_VERIFY_PAN").is_some() {
                    tasks.push(cosmic::task::future(async {
                        tokio::time::sleep(Duration::from_millis(3500)).await;
                        Message::VerifyNudgePan
                    }));
                }
                return Task::batch(tasks);
            }
            Message::VerifyExpandRadar => {
                if self.radar_expanded {
                    return Task::none();
                }
                let Some(id) = self.core.main_window_id().or(self.popup) else {
                    return Task::none();
                };
                return window::monitor_size(id)
                    .map(|size| cosmic::Action::App(Message::RadarExpandWith(size)));
            }
            Message::VerifyNudgePan => {
                self.pan_x += 120.0;
                self.gesturing = true;
                return self.settle_now();
            }
            Message::RefreshWeather => {
                if self.config.has_weather_location() {
                    return self.start_weather_load(false);
                }
            }
            Message::ZipDraft(value) => {
                self.zip_draft = value;
                self.geocode_error = None;
                self.geocode_hits.clear();
                self.geocode_hits_query.clear();
                let trimmed = self.zip_draft.trim();
                if trimmed.is_empty() || is_zip_lookup(trimmed) {
                    self.geocode_loading = false;
                    return Task::none();
                }
                self.geocode_generation = self.geocode_generation.wrapping_add(1);
                let generation = self.geocode_generation;
                return cosmic::task::future(async move {
                    tokio::time::sleep(GEOCODE_SEARCH_DEBOUNCE).await;
                    Message::GeocodeSearch { generation }
                });
            }
            Message::GeocodeSearch { generation } => {
                if generation != self.geocode_generation {
                    return Task::none();
                }
                return self.run_geocode_search(generation);
            }
            Message::SaveZip => return self.save_location(),
            Message::GeocodeReady { generation, hits } => {
                if generation != self.geocode_generation {
                    return Task::none();
                }
                self.geocode_loading = false;
                self.geocode_hits = hits;
                self.geocode_hits_query = self.zip_draft.trim().to_string();
                if self.geocode_hits.is_empty() && !is_zip_lookup(&self.zip_draft) {
                    self.geocode_error = Some("no matching US cities found".into());
                }
            }
            Message::GeocodeFailed {
                generation,
                message,
            } => {
                if generation != self.geocode_generation {
                    return Task::none();
                }
                self.geocode_loading = false;
                self.geocode_hits.clear();
                self.geocode_hits_query.clear();
                self.geocode_error = Some(message);
            }
            Message::PickGeocode(index) => {
                let trimmed = self.zip_draft.trim();
                if self.geocode_hits_query != trimmed {
                    return Task::none();
                }
                let Some(hit) = self.geocode_hits.get(index).cloned() else {
                    return Task::none();
                };
                let place = place_from_geocode_hit(&hit);
                self.config.apply_place(&place);
                if is_us_zip(&place.zip) {
                    self.zip_draft.clone_from(&place.zip);
                } else {
                    self.zip_draft = format!("{}, {}", place.city, place.state_abbreviation);
                }
                self.geocode_hits.clear();
                self.geocode_hits_query.clear();
                self.geocode_error = None;
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
            Message::CycleRadarSpeed => {
                self.radar_speed = self.radar_speed.next();
            }
            Message::CycleRadarLookback => {
                self.radar_lookback = self.radar_lookback.next();
                self.rebuild_radar_frames();
                return self.apply_lookback_change();
            }
            Message::FrameSlider(index) => {
                self.playing = false;
                if let Some(offset) = self.frame_offset_at(index as usize) {
                    self.shown_offset = offset;
                    if self.gesturing {
                        return self.schedule_settle();
                    }
                    return self.settle_now();
                }
            }
            Message::AdvanceRadar => return self.advance_radar(),
            Message::ZoomIn => return self.nudge_zoom(1.0),
            Message::ZoomOut => return self.nudge_zoom(-1.0),
            Message::Recenter => {
                self.pan_x = 0.0;
                self.pan_y = 0.0;
                let (view_width, view_height) = self.map_size();
                self.relief_camera = default_relief_camera(view_width, view_height);
                return self.paint_gesture();
            }
            Message::ToggleRadarMode => {
                self.radar_mode = match self.radar_mode {
                    RadarMode::Flat => RadarMode::Relief,
                    RadarMode::Relief => RadarMode::Flat,
                };
                if self.radar_mode == RadarMode::Relief {
                    let (view_width, view_height) = self.map_size();
                    self.relief_camera = default_relief_camera(view_width, view_height);
                }
                if self.gesturing {
                    return self.schedule_settle();
                }
                return self.settle_now();
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
                    self.drag = Some(self.begin_drag(point, self.shift_held));
                }
            }
            Message::MapRightPress => {
                if self.active_tab != PopupTab::Radar
                    || self.radar_mode != RadarMode::Relief
                    || !self.map_hovered
                {
                    return Task::none();
                }
                if let Some(point) = self.pointer {
                    self.drag = Some(self.begin_drag(point, true));
                }
            }
            Message::MapRelease => {
                let was_pan = matches!(self.drag, Some(DragState::Pan { .. }));
                let was_orbit = matches!(self.drag, Some(DragState::Orbit { .. }));
                self.drag = None;
                if was_pan {
                    return self.schedule_settle();
                }
                if was_orbit {
                    self.present_current_snapshot();
                }
            }
            Message::MapLeave => {
                self.map_hovered = false;
                let was_pan = matches!(self.drag, Some(DragState::Pan { .. }));
                self.drag = None;
                if was_pan {
                    return self.schedule_settle();
                }
            }
            Message::MapMove(point) => {
                self.pointer = Some(point);
                self.map_hovered = true;
                match self.drag {
                    Some(DragState::Pan {
                        start,
                        pan_x,
                        pan_y,
                    }) => {
                        self.pan_x = pan_x + f64::from(point.x - start.x);
                        self.pan_y = pan_y + f64::from(point.y - start.y);
                        // Painted from cache at the live camera, coalesced to
                        // one frame so pointer motion stays ahead of compose.
                        return self.paint_gesture();
                    }
                    Some(DragState::Orbit { start, yaw, pitch }) => {
                        self.relief_camera.yaw = yaw + (point.x - start.x) * 0.005;
                        self.relief_camera.pitch =
                            clamp_relief_pitch(pitch + (point.y - start.y) * 0.004);
                        return self.paint_gesture();
                    }
                    None => {}
                }
            }
            Message::MapScroll(delta) => return self.handle_scroll(delta),
            Message::MapSettle(token) => {
                if token == self.settle_token {
                    self.gesturing = false;
                    self.apply_deferred_shown_offset();
                    return self.refresh_visible_map();
                }
            }
            Message::GestureCompose => {
                self.gesture_paint_scheduled = false;
                if self.gesturing {
                    self.gesture_paint_at = Some(Instant::now());
                    self.paint_gesture_frame();
                }
            }
            Message::PrefetchDone { generation, cache } => {
                if prefetch_absorb_allowed(generation, self.map_generation) {
                    self.tile_cache.absorb(&cache);
                }
            }
            Message::ScaleFactor(scale) => {
                let next = scale.clamp(1.0, 2.0);
                if (next - self.pixel_ratio).abs() > 0.01 {
                    self.pixel_ratio = next;
                    if self.map_handle.is_some() && !self.gesturing {
                        return self.settle_now();
                    }
                }
            }
            Message::Modifiers(modifiers) => {
                self.ctrl_held = modifiers.control();
                self.shift_held = modifiers.shift();
            }
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
                self.selected_forecast_day = None;
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
                let presented = self.apply_frame(*frame, zoom, pan_x, pan_y);
                let next = self.load_next_frame();
                if presented {
                    let prefetch = self.schedule_prefetch();
                    return Task::batch([next, prefetch]);
                }
                if !self.camera_matches(zoom, pan_x, pan_y) {
                    return Task::batch([next, self.schedule_settle()]);
                }
                // Playback may have moved shown_offset while this fetch ran.
                // Paint the live offset from the tiles this response just cached.
                if self.recompose_current() {
                    let prefetch = self.schedule_prefetch();
                    return Task::batch([next, prefetch]);
                }
                return next;
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
            Message::LookbackBatch {
                generation,
                newly_ready,
                attempted,
                cache,
            } => {
                self.tile_cache.absorb(&cache);
                // Always clear so a bumped map_generation cannot leave fetch stuck.
                self.lookback_batch_inflight = false;
                if generation != self.map_generation {
                    return Task::none();
                }
                for offset in attempted {
                    self.finish_pending(offset);
                }
                for offset in newly_ready {
                    if !self.frames_ready.contains(&offset) {
                        self.frames_ready.push(offset);
                    }
                }
                self.frames_ready.sort_unstable();
                if !self.gesturing {
                    let _ = self.recompose_current();
                }
            }
            Message::SelectForecastDay(index) => {
                if self
                    .forecast
                    .as_ref()
                    .is_some_and(|forecast| index < forecast.daily.len())
                {
                    self.selected_forecast_day = if self.selected_forecast_day == Some(index) {
                        None
                    } else {
                        Some(index)
                    };
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
        if self.forecast.is_none() && self.config.has_weather_location() && !self.weather_loading {
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
        let (view_width, view_height) = self.map_size();
        self.settled_width = view_width;
        self.settled_height = view_height;
        self.settle_now()
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
        if !self.config.has_weather_location() {
            self.weather_loading = false;
            self.place = None;
            self.forecast = None;
            self.alerts.clear();
            self.map_error = Some(coldfront_core::Error::InvalidZip.to_string());
            return Task::none();
        }
        let zip = self.config.zip.clone();
        let city = self.config.city.clone();
        let state_abbreviation = self.config.state_abbreviation.clone();
        let latitude = self.config.latitude;
        let longitude = self.config.longitude;
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
            match mapload::load_weather_from_config(
                &client,
                &zip,
                &city,
                &state_abbreviation,
                latitude,
                longitude,
            )
            .await
            {
                Ok(bundle) => Message::WeatherReady(Box::new(WeatherLoaded { generation, bundle })),
                Err(error) => Message::WeatherFailed {
                    generation,
                    message: error.to_string(),
                },
            }
        })
    }

    fn run_geocode_search(&mut self, generation: u64) -> Task<cosmic::Action<Message>> {
        let trimmed = self.zip_draft.trim().to_string();
        if is_zip_lookup(&trimmed) || trimmed.is_empty() || !geocode_query_allowed(&trimmed) {
            self.geocode_hits.clear();
            self.geocode_hits_query.clear();
            self.geocode_loading = false;
            if !trimmed.is_empty() && !is_zip_lookup(&trimmed) && !geocode_query_allowed(&trimmed) {
                self.geocode_error = Some("no matching US cities found".into());
            } else {
                self.geocode_error = None;
            }
            return Task::none();
        }
        self.geocode_loading = true;
        self.geocode_hits.clear();
        self.geocode_hits_query.clear();
        cosmic::task::future(async move {
            let client = match HttpClient::new() {
                Ok(client) => client,
                Err(error) => {
                    return Message::GeocodeFailed {
                        generation,
                        message: error.to_string(),
                    };
                }
            };
            match client.search_cities(&trimmed).await {
                Ok(hits) => Message::GeocodeReady { generation, hits },
                Err(error) => Message::GeocodeFailed {
                    generation,
                    message: error.to_string(),
                },
            }
        })
    }

    fn save_location(&mut self) -> Task<cosmic::Action<Message>> {
        let trimmed = self.zip_draft.trim().to_string();
        if trimmed.is_empty() {
            self.map_error = Some(coldfront_core::Error::InvalidZip.to_string());
            return Task::none();
        }
        if is_us_zip(&trimmed) {
            self.config.zip = trimmed;
            self.config.clear_resolved_place();
            config::save(&self.config);
            self.geocode_hits.clear();
            self.geocode_error = None;
            return self.start_weather_load(true);
        }
        let hits_current = self.geocode_hits_query == trimmed;
        if hits_current && self.geocode_hits.len() == 1 {
            let place = place_from_geocode_hit(&self.geocode_hits[0]);
            self.config.apply_place(&place);
            config::save(&self.config);
            self.geocode_hits.clear();
            self.geocode_hits_query.clear();
            self.geocode_error = None;
            return self.start_weather_load(true);
        }
        if hits_current && self.geocode_hits.len() > 1 {
            self.geocode_error = Some("choose a city from the list".into());
            return Task::none();
        }
        self.geocode_error = Some("no matching US cities found".into());
        Task::none()
    }

    fn reset_map_state(&mut self) {
        self.map_generation = self.map_generation.wrapping_add(1);
        self.tile_cache.clear();
        self.map_handle = None;
        self.frames_ready.clear();
        self.pending_offsets.clear();
        self.lookback_batch_inflight = false;
        self.radar_tiles = 0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.zoom = 7.0;
        self.settled_zoom = 7.0;
        self.settled_pan_x = 0.0;
        self.settled_pan_y = 0.0;
        self.settled_width = MAP_VIEW_WIDTH;
        self.settled_height = MAP_VIEW_HEIGHT;
        self.gesturing = false;
        self.shown_offset = 0;
        self.radar_opened = false;
        self.map_loading = false;
        self.relief_ground = None;
        self.relief_field = None;
        self.relief_empty = false;
        self.relief_camera = default_relief_camera(MAP_VIEW_WIDTH, MAP_VIEW_HEIGHT);
        self.rebuild_radar_frames();
    }

    fn rebuild_radar_frames(&mut self) {
        self.radar_frames = applet_radar_frames_lookback(self.radar_lookback);
    }

    fn reload_map(&mut self, clear_cache: bool) -> Task<cosmic::Action<Message>> {
        let Some(place) = self.place.clone() else {
            return Task::none();
        };
        self.map_generation = self.map_generation.wrapping_add(1);
        let generation = self.map_generation;
        self.lookback_batch_inflight = false;
        self.rebuild_radar_frames();
        if clear_cache {
            self.tile_cache.clear();
            self.frames_ready.clear();
            self.map_handle = None;
            self.relief_ground = None;
            self.relief_field = None;
            self.relief_empty = false;
        }
        // Keep the last composite on screen while edge tiles refill after pan.
        self.map_loading = self.map_handle.is_none();
        self.map_error = None;
        let kind = basemap_kind(&self.config);
        let zoom = self.zoom;
        let pan_x = self.pan_x;
        let pan_y = self.pan_y;
        let (view_width, view_height) = self.map_size();
        let pixel_ratio = self.pixel_ratio;
        let shown_offset = self.shown_offset;
        let frames = self.radar_frames.clone();
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
                pixel_ratio,
                offset_minutes: first_offset,
            };
            match mapload::ensure_map_frame(&client, &mut cache, &kind, &request, &frames).await {
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
        let pixel_ratio = self.pixel_ratio;
        let frames = self.radar_frames.clone();
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
                pixel_ratio,
                offset_minutes: offset,
            };
            match mapload::ensure_map_frame(&client, &mut cache, &kind, &request, &frames).await {
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

    fn camera_matches(&self, zoom: f64, pan_x: f64, pan_y: f64) -> bool {
        (zoom - self.zoom).abs() < 1e-4
            && (pan_x - self.pan_x).abs() < 0.5
            && (pan_y - self.pan_y).abs() < 0.5
    }

    /// Present a settled frame when the camera still matches. Returns true when shown.
    fn apply_frame(&mut self, frame: LoadedFrame, zoom: f64, pan_x: f64, pan_y: f64) -> bool {
        if frame.radar_tiles > 0 && !self.frames_ready.contains(&frame.offset_minutes) {
            self.frames_ready.push(frame.offset_minutes);
            self.frames_ready.sort_unstable();
        }
        if self.frames_ready.len() == 1 && frame.radar_tiles > 0 {
            self.shown_offset = frame.offset_minutes;
        }
        if !self.camera_matches(zoom, pan_x, pan_y) {
            // Stale camera: keep the live snapshot; caller will settle again.
            return false;
        }
        if self.shown_offset != frame.offset_minutes {
            return false;
        }
        if frame.radar_tiles == 0 && self.map_handle.is_some() {
            // Keep the previous snapshot; do not paint pale holes over it.
            return false;
        }
        if self.radar_mode == RadarMode::Relief {
            self.recompose_current();
            return self.map_handle.is_some();
        }
        self.radar_tiles = frame.radar_tiles;
        self.present_snapshot(frame.width, frame.height, frame.pixels, zoom, pan_x, pan_y);
        true
    }

    fn present_snapshot(
        &mut self,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        zoom: f64,
        pan_x: f64,
        pan_y: f64,
    ) {
        let (view_width, view_height) = self.map_size();
        self.settled_zoom = zoom;
        self.settled_pan_x = pan_x;
        self.settled_pan_y = pan_y;
        self.settled_width = view_width;
        self.settled_height = view_height;
        self.map_handle = Some(cosmic::iced::widget::image::Handle::from_rgba(
            width, height, pixels,
        ));
    }

    fn present_current_snapshot(&mut self) {
        let Some(handle) = self.map_handle.clone() else {
            return;
        };
        let (view_width, view_height) = self.map_size();
        self.settled_zoom = self.zoom;
        self.settled_pan_x = self.pan_x;
        self.settled_pan_y = self.pan_y;
        self.settled_width = view_width;
        self.settled_height = view_height;
        self.map_handle = Some(handle);
    }

    fn begin_drag(&self, point: Point, orbit: bool) -> DragState {
        if orbit && self.radar_mode == RadarMode::Relief {
            DragState::Orbit {
                start: point,
                yaw: self.relief_camera.yaw,
                pitch: self.relief_camera.pitch,
            }
        } else {
            DragState::Pan {
                start: point,
                pan_x: self.pan_x,
                pan_y: self.pan_y,
            }
        }
    }

    /// Paint the shown offset from `tile_cache` at the live camera. Returns
    /// false when composition fails. During pan/zoom, still paint when radar
    /// tiles are missing so the stage stays filled (pale holes beat sliding
    /// the old bitmap off-screen).
    fn recompose_current(&mut self) -> bool {
        self.recompose_at_live_camera(self.gesturing)
    }

    fn recompose_at_live_camera(&mut self, fill_when_empty_radar: bool) -> bool {
        let Some(place) = &self.place else {
            return false;
        };
        let (view_width, view_height) = self.map_size();
        // Gesture paints render below display resolution: a fullscreen frame at
        // the display pixel ratio costs more than a frame budget to resample, so
        // the canvas scales the gesture frame up until settle repaints sharp.
        let render_scale = if self.gesturing {
            gesture_render_scale(view_width, view_height)
        } else {
            1.0
        };
        let render_width = ((f64::from(view_width) * render_scale).round() as u32).max(1);
        let render_height = ((f64::from(view_height) * render_scale).round() as u32).max(1);
        let (viewport, frac_scale) = mapload::viewport_for(
            place.latitude,
            place.longitude,
            self.zoom,
            self.pan_x * render_scale,
            self.pan_y * render_scale,
            render_width,
            render_height,
        );
        let kind = basemap_kind(&self.config);
        // One device pixel per logical pixel while moving: on HiDPI that is a
        // quarter of the resample and upload of the settled frame.
        let pixel_ratio = if self.gesturing {
            1.0
        } else {
            self.pixel_ratio
        };
        let present = mapload::PresentScale {
            frac_scale,
            view_width: render_width,
            view_height: render_height,
            pixel_ratio,
        };
        if self.radar_mode == RadarMode::Relief {
            let Ok((ground, radar, radar_tiles)) = mapload::compose_relief_layers(
                &self.tile_cache,
                &kind,
                &viewport,
                present,
                self.shown_offset,
            ) else {
                return false;
            };
            if radar_tiles == 0 && self.map_handle.is_some() && !fill_when_empty_radar {
                return false;
            }
            let (cols, rows) = echo_grid(radar.width, radar.height);
            let echoes = sample_echoes(&radar.pixels, radar.width, radar.height, cols, rows);
            self.relief_empty = echoes.is_empty();
            let field = build_echo_field(&echoes, cols, rows);
            self.radar_tiles = radar_tiles;
            self.relief_ground.replace(ground.clone());
            self.relief_field.replace(field.clone());
            let painted = render_relief(&ground, &field, &self.relief_camera);
            self.present_snapshot(
                painted.width,
                painted.height,
                painted.pixels,
                self.zoom,
                self.pan_x,
                self.pan_y,
            );
            return true;
        }
        self.relief_empty = false;
        let Ok(frame) = mapload::compose_cached(
            &self.tile_cache,
            &kind,
            &viewport,
            present,
            self.shown_offset,
        ) else {
            return false;
        };
        if frame.radar_tiles == 0 && self.map_handle.is_some() && !fill_when_empty_radar {
            return false;
        }
        self.radar_tiles = frame.radar_tiles;
        self.present_snapshot(
            frame.width,
            frame.height,
            frame.pixels,
            self.zoom,
            self.pan_x,
            self.pan_y,
        );
        true
    }

    fn paint_relief_from_cache(&mut self) {
        let (Some(ground), Some(field)) = (&self.relief_ground, &self.relief_field) else {
            self.recompose_current();
            return;
        };
        let painted = render_relief(ground, field, &self.relief_camera);
        let (view_width, view_height) = self.map_size();
        self.settled_width = view_width;
        self.settled_height = view_height;
        self.map_handle = Some(cosmic::iced::widget::image::Handle::from_rgba(
            painted.width,
            painted.height,
            painted.pixels,
        ));
    }

    /// Fetch tiles for the current camera without blanking the shown map.
    ///
    /// First load still goes through `reload_map`. Later pan/zoom settles fill
    /// edge tiles for the shown frame and every frame already ready, then
    /// present once the camera is still.
    fn refresh_visible_map(&mut self) -> Task<cosmic::Action<Message>> {
        if self.map_handle.is_none() || self.frames_ready.is_empty() {
            return self.reload_map(false);
        }
        let Some(place) = self.place.clone() else {
            return Task::none();
        };
        self.map_generation = self.map_generation.wrapping_add(1);
        let generation = self.map_generation;
        self.lookback_batch_inflight = false;
        self.map_error = None;
        let kind = basemap_kind(&self.config);
        let zoom = self.zoom;
        let pan_x = self.pan_x;
        let pan_y = self.pan_y;
        let (view_width, view_height) = self.map_size();
        let pixel_ratio = self.pixel_ratio;
        let shown_offset = self.shown_offset;
        let frames = self.radar_frames.clone();
        let extra_offsets: Vec<u32> = self
            .frames_ready
            .iter()
            .copied()
            .filter(|offset| *offset != shown_offset)
            .collect();
        // Same order as reload_map: `.last()` loads next, so reverse chrono.
        let pending: Vec<u32> = self
            .radar_frames
            .iter()
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
                pixel_ratio,
                offset_minutes: shown_offset,
            };
            match mapload::ensure_map_frames(
                &client,
                &mut cache,
                &kind,
                &request,
                &extra_offsets,
                &frames,
            )
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

    fn advance_radar(&mut self) -> Task<cosmic::Action<Message>> {
        let full: Vec<u32> = self
            .radar_frames
            .iter()
            .map(|frame| frame.offset_minutes)
            .collect();
        let ready: std::collections::HashSet<u32> = self.frames_ready.iter().copied().collect();
        let pending: std::collections::HashSet<u32> =
            self.pending_offsets.iter().copied().collect();
        match next_playback_step(&full, &ready, &pending, self.shown_offset) {
            PlaybackStep::Wait => {
                if !self.pending_offsets.is_empty() && !self.lookback_batch_inflight {
                    return self.load_lookback_batch();
                }
                Task::none()
            }
            PlaybackStep::Stay => Task::none(),
            PlaybackStep::Advance(next_offset) => {
                let (next_shown, deferred) =
                    playback_advance_during_gesture(self.gesturing, self.shown_offset, next_offset);
                if let Some(offset) = deferred {
                    self.deferred_shown_offset = Some(offset);
                    return self.schedule_settle();
                }
                self.shown_offset = next_shown;
                // Cache-only paint. settle_now bumps map_generation and cancels in-flight
                // fullscreen refreshes, which freezes playback on large maps.
                self.gesturing = false;
                if self.recompose_current() {
                    return Task::none();
                }
                // No tiles at this camera yet (pan/zoom before lookback settle).
                self.schedule_settle()
            }
        }
    }

    fn apply_deferred_shown_offset(&mut self) {
        if let Some(offset) = self.deferred_shown_offset.take() {
            self.shown_offset = offset;
        }
    }

    fn nudge_zoom(&mut self, steps: f64) -> Task<cosmic::Action<Message>> {
        let (view_width, view_height) = self.map_size();
        let origin_x = f64::from(view_width) / 2.0;
        let origin_y = f64::from(view_height) / 2.0;
        let next = zoom_toward(
            GestureCamera {
                zoom: self.zoom,
                pan_x: self.pan_x,
                pan_y: self.pan_y,
            },
            self.zoom + steps,
            origin_x,
            origin_y,
            f64::from(view_width),
            f64::from(view_height),
        );
        if (next.zoom - self.zoom).abs() < 1e-6 {
            return Task::none();
        }
        self.zoom = next.zoom;
        self.pan_x = next.pan_x;
        self.pan_y = next.pan_y;
        self.paint_gesture()
    }

    fn handle_scroll(&mut self, delta: ScrollDelta) -> Task<cosmic::Action<Message>> {
        let (view_width, view_height) = self.map_size();
        let width = f64::from(view_width);
        let height = f64::from(view_height);
        let (origin_x, origin_y) = self
            .pointer
            .map(|point| (f64::from(point.x), f64::from(point.y)))
            .unwrap_or((width / 2.0, height / 2.0));
        match delta {
            ScrollDelta::Lines { y, .. } => {
                if y == 0.0 {
                    return Task::none();
                }
                let pixels = f64::from(y) * 16.0;
                let next = zoom_toward(
                    GestureCamera {
                        zoom: self.zoom,
                        pan_x: self.pan_x,
                        pan_y: self.pan_y,
                    },
                    self.zoom - pixels * 0.008,
                    origin_x,
                    origin_y,
                    width,
                    height,
                );
                self.zoom = next.zoom;
                self.pan_x = next.pan_x;
                self.pan_y = next.pan_y;
                self.paint_gesture()
            }
            ScrollDelta::Pixels { x, y } => {
                if self.ctrl_held {
                    if y == 0.0 {
                        return Task::none();
                    }
                    let next = zoom_toward(
                        GestureCamera {
                            zoom: self.zoom,
                            pan_x: self.pan_x,
                            pan_y: self.pan_y,
                        },
                        self.zoom - f64::from(y) * 0.008,
                        origin_x,
                        origin_y,
                        width,
                        height,
                    );
                    self.zoom = next.zoom;
                    self.pan_x = next.pan_x;
                    self.pan_y = next.pan_y;
                    self.gesturing = true;
                    let _ = self.recompose_current();
                    self.schedule_settle()
                } else {
                    self.pan_x += f64::from(x);
                    self.pan_y += f64::from(y);
                    self.paint_gesture()
                }
            }
        }
    }

    /// Repaint the live gesture camera from cache, coalesced to one paint per
    /// [`GESTURE_FRAME`].
    ///
    /// Pointer motion arrives far faster than a fullscreen compose can run, so a
    /// paint per event backs up the event loop and the map falls behind the
    /// pointer. Between paints `board_transform` slides the last frame, which
    /// keeps the motion immediate; the scheduled paint then catches up.
    fn paint_gesture(&mut self) -> Task<cosmic::Action<Message>> {
        self.gesturing = true;
        let settle = self.schedule_settle();
        if self.gesture_paint_scheduled {
            return settle;
        }
        let now = Instant::now();
        if gesture_paint_due(self.gesture_paint_at, now) {
            self.gesture_paint_at = Some(now);
            self.paint_gesture_frame();
            return settle;
        }
        self.gesture_paint_scheduled = true;
        let wait = self
            .gesture_paint_at
            .map(|last| GESTURE_FRAME.saturating_sub(now.duration_since(last)))
            .unwrap_or_default();
        Task::batch([
            settle,
            cosmic::task::future(async move {
                tokio::time::sleep(wait).await;
                Message::GestureCompose
            }),
        ])
    }

    /// Paint one gesture frame: relief orbit re-renders the heightfield, every
    /// other gesture recomposes the viewport.
    fn paint_gesture_frame(&mut self) {
        if matches!(self.drag, Some(DragState::Orbit { .. })) {
            self.paint_relief_from_cache();
        } else {
            let _ = self.recompose_current();
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

    fn settle_now(&mut self) -> Task<cosmic::Action<Message>> {
        self.gesturing = false;
        self.settle_token = self.settle_token.wrapping_add(1);
        self.apply_deferred_shown_offset();
        self.refresh_visible_map()
    }

    fn schedule_prefetch(&mut self) -> Task<cosmic::Action<Message>> {
        let Some(place) = self.place.clone() else {
            return Task::none();
        };
        let kind = basemap_kind(&self.config);
        let zoom = self.zoom;
        let pan_x = self.pan_x;
        let pan_y = self.pan_y;
        let (view_width, view_height) = self.map_size();
        let pixel_ratio = self.pixel_ratio;
        let shown_offset = self.shown_offset;
        let full: Vec<u32> = self
            .radar_frames
            .iter()
            .map(|frame| frame.offset_minutes)
            .collect();
        let ready: std::collections::HashSet<u32> = self.frames_ready.iter().copied().collect();
        let pending: std::collections::HashSet<u32> =
            self.pending_offsets.iter().copied().collect();
        let next_offset = match next_playback_step(&full, &ready, &pending, shown_offset) {
            PlaybackStep::Advance(offset) => Some(offset),
            PlaybackStep::Wait | PlaybackStep::Stay => None,
        };
        let generation = self.map_generation;
        let frames = self.radar_frames.clone();
        let mut cache = self.tile_cache.clone();
        cosmic::task::future(async move {
            let Ok(client) = HttpClient::new() else {
                return Message::PrefetchDone {
                    generation,
                    cache: Box::new(cache),
                };
            };
            let request = mapload::MapRequest {
                latitude: place.latitude,
                longitude: place.longitude,
                zoom,
                pan_x,
                pan_y,
                view_width,
                view_height,
                pixel_ratio,
                offset_minutes: shown_offset,
            };
            mapload::prefetch_around(&client, &mut cache, &kind, &request, &frames, next_offset)
                .await;
            Message::PrefetchDone {
                generation,
                cache: Box::new(cache),
            }
        })
    }

    /// Affine from the settled snapshot to the live camera for the radar canvas.
    pub(crate) fn board_transform(&self) -> SnapshotTransform {
        let (view_width, view_height) = self.map_size();
        snapshot_transform(
            GestureCamera {
                zoom: self.settled_zoom,
                pan_x: self.settled_pan_x,
                pan_y: self.settled_pan_y,
            },
            GestureCamera {
                zoom: self.zoom,
                pan_x: self.pan_x,
                pan_y: self.pan_y,
            },
            f64::from(view_width),
            f64::from(view_height),
        )
    }

    pub(crate) fn settled_map_size(&self) -> (u32, u32) {
        (self.settled_width, self.settled_height)
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
        self.radar_frames
            .iter()
            .position(|frame| frame.offset_minutes == self.shown_offset)
            .unwrap_or(0) as u32
    }

    pub(crate) fn frame_count(&self) -> u32 {
        self.radar_frames.len().saturating_sub(1) as u32
    }

    pub(crate) fn radar_speed_label(&self) -> String {
        format!("{}×", self.radar_speed.multiplier())
    }

    pub(crate) fn radar_lookback_label(&self) -> String {
        format!("{}h", self.radar_lookback.hours())
    }

    fn frame_offset_at(&self, index: usize) -> Option<u32> {
        self.radar_frames
            .get(index)
            .map(|frame| frame.offset_minutes)
    }

    /// Grow or shrink the playback list without clearing the tile cache.
    fn apply_lookback_change(&mut self) -> Task<cosmic::Action<Message>> {
        let allowed: Vec<u32> = self
            .radar_frames
            .iter()
            .map(|frame| frame.offset_minutes)
            .collect();
        let (pending, ready, shown) = merge_lookback_offsets(
            &allowed,
            &self.pending_offsets,
            &self.frames_ready,
            self.shown_offset,
        );
        self.pending_offsets = pending;
        self.frames_ready = ready;
        let shown_changed = shown != self.shown_offset;
        self.shown_offset = shown;
        if !self.radar_opened || self.place.is_none() {
            return Task::none();
        }
        if shown_changed {
            if self.gesturing {
                return self.schedule_settle();
            }
            return self.settle_now();
        }
        if self.pending_offsets.is_empty() {
            return Task::none();
        }
        self.load_lookback_batch()
    }

    /// Fetch every pending lookback offset at the current camera in one pass.
    fn load_lookback_batch(&mut self) -> Task<cosmic::Action<Message>> {
        if self.pending_offsets.is_empty() || self.lookback_batch_inflight {
            return Task::none();
        }
        let Some(place) = self.place.clone() else {
            return Task::none();
        };
        self.lookback_batch_inflight = true;
        let generation = self.map_generation;
        let kind = basemap_kind(&self.config);
        let zoom = self.zoom;
        let pan_x = self.pan_x;
        let pan_y = self.pan_y;
        let (view_width, view_height) = self.map_size();
        let pixel_ratio = self.pixel_ratio;
        let shown_offset = self.shown_offset;
        let frames = self.radar_frames.clone();
        let attempted = self.pending_offsets.clone();
        let mut cache = self.tile_cache.clone();
        cosmic::task::future(async move {
            let Ok(client) = HttpClient::new() else {
                return Message::LookbackBatch {
                    generation,
                    newly_ready: Vec::new(),
                    attempted,
                    cache: Box::new(cache),
                };
            };
            let request = mapload::MapRequest {
                latitude: place.latitude,
                longitude: place.longitude,
                zoom,
                pan_x,
                pan_y,
                view_width,
                view_height,
                pixel_ratio,
                offset_minutes: shown_offset,
            };
            let _ = mapload::ensure_map_frames(
                &client, &mut cache, &kind, &request, &attempted, &frames,
            )
            .await;
            let (viewport, frac_scale) = mapload::viewport_for(
                place.latitude,
                place.longitude,
                zoom,
                pan_x,
                pan_y,
                view_width,
                view_height,
            );
            let present = mapload::PresentScale {
                frac_scale,
                view_width,
                view_height,
                pixel_ratio,
            };
            let mut newly_ready = Vec::new();
            for offset in &attempted {
                if let Ok(frame) =
                    mapload::compose_cached(&cache, &kind, &viewport, present, *offset)
                    && frame.radar_tiles > 0
                {
                    newly_ready.push(*offset);
                }
            }
            Message::LookbackBatch {
                generation,
                newly_ready,
                attempted,
                cache: Box::new(cache),
            }
        })
    }

    pub(crate) fn current_frame_label(&self) -> String {
        frame_label(self.shown_offset)
    }

    pub(crate) fn current_frame_clock(&self) -> String {
        let instant = self
            .radar_frames
            .iter()
            .find(|frame| frame.offset_minutes == self.shown_offset)
            .or_else(|| self.radar_frames.last())
            .map(|frame| frame.instant);
        let Some(instant) = instant else {
            return String::new();
        };
        let zone = self
            .forecast
            .as_ref()
            .map(|forecast| forecast.timezone.as_str());
        format_radar_clock(instant, zone)
    }

    pub(crate) fn map_size(&self) -> (u32, u32) {
        if self.radar_expanded {
            let size = self.popup_size();
            // Timeline sits on the map. Reserve header, tabs, and caption only.
            (
                (size.width - 48.0).max(400.0) as u32,
                (size.height - 200.0).max(240.0) as u32,
            )
        } else {
            (MAP_VIEW_WIDTH, MAP_VIEW_HEIGHT)
        }
    }

    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// True while a gesture frame is painted below display resolution, so the
    /// canvas should smooth it while scaling back up.
    pub(crate) fn gesture_low_res(&self) -> bool {
        if !self.gesturing {
            return false;
        }
        let (view_width, view_height) = self.map_size();
        self.pixel_ratio > 1.0 || gesture_render_scale(view_width, view_height) < 1.0
    }

    pub(crate) fn current_frame_caption(&self) -> String {
        let mut label = self.current_frame_label();
        if self.radar_mode == RadarMode::Relief {
            label.push_str(" · ");
            label.push_str(&fl!("radar-relief"));
        }
        label
    }
}

fn frame_label(offset: u32) -> String {
    if offset == 0 {
        fl!("latest-scan")
    } else {
        fl!("minutes-ago", minutes = i64::from(offset))
    }
}

/// Merge pending and ready offsets when lookback changes.
///
/// Growing appends missing offsets behind the current pending queue (latest-first
/// via the allowed list order). Shrinking drops offsets outside the new list.
pub(crate) fn merge_lookback_offsets(
    allowed: &[u32],
    pending: &[u32],
    ready: &[u32],
    shown_offset: u32,
) -> (Vec<u32>, Vec<u32>, u32) {
    let allowed_set: std::collections::HashSet<u32> = allowed.iter().copied().collect();
    let ready: Vec<u32> = ready
        .iter()
        .copied()
        .filter(|offset| allowed_set.contains(offset))
        .collect();
    let mut pending: Vec<u32> = pending
        .iter()
        .copied()
        .filter(|offset| allowed_set.contains(offset))
        .collect();
    // Allowed is oldest→newest; reverse so `.last()` loads nearer frames first.
    for offset in allowed.iter().rev().copied() {
        if ready.contains(&offset) || pending.contains(&offset) {
            continue;
        }
        pending.insert(0, offset);
    }
    let shown = if allowed_set.contains(&shown_offset) {
        shown_offset
    } else {
        allowed.last().copied().unwrap_or(0)
    };
    // Do not queue the frame already on screen.
    pending.retain(|offset| *offset != shown);
    (pending, ready, shown)
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

fn location_draft(config: &Config) -> String {
    if !config.city.is_empty() {
        if config.state_abbreviation.is_empty() {
            return config.city.clone();
        }
        return format!("{}, {}", config.city, config.state_abbreviation);
    }
    config.zip.clone()
}

fn location_changed(previous: &Config, next: &Config) -> bool {
    previous.zip != next.zip
        || previous.city != next.city
        || previous.state_abbreviation != next.state_abbreviation
        || previous.latitude != next.latitude
        || previous.longitude != next.longitude
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

pub(crate) fn prefetch_absorb_allowed(task_generation: u64, current_generation: u64) -> bool {
    task_generation == current_generation
}

pub(crate) fn playback_advance_during_gesture(
    gesturing: bool,
    shown_offset: u32,
    next_offset: u32,
) -> (u32, Option<u32>) {
    if gesturing {
        (shown_offset, Some(next_offset))
    } else {
        (next_offset, None)
    }
}

/// True when a gesture may paint now: immediately when nothing has painted yet,
/// otherwise once [`GESTURE_FRAME`] has passed since the last paint.
pub(crate) fn gesture_paint_due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.duration_since(last) >= GESTURE_FRAME)
}

/// Linear scale for a gesture frame: 1.0 when the view already fits
/// [`GESTURE_PIXEL_BUDGET`], otherwise the factor that fits it.
pub(crate) fn gesture_render_scale(view_width: u32, view_height: u32) -> f64 {
    let pixels = f64::from(view_width.max(1)) * f64::from(view_height.max(1));
    if pixels <= GESTURE_PIXEL_BUDGET {
        return 1.0;
    }
    (GESTURE_PIXEL_BUDGET / pixels).sqrt().clamp(0.5, 1.0)
}

/// Next loop step over the full lookback list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaybackStep {
    Advance(u32),
    Wait,
    Stay,
}

/// Advance onto the next ready offset. Wait while the next is still pending.
/// Skip permanently failed offsets (neither ready nor pending).
pub(crate) fn next_playback_step(
    full: &[u32],
    ready: &std::collections::HashSet<u32>,
    pending: &std::collections::HashSet<u32>,
    shown_offset: u32,
) -> PlaybackStep {
    if full.len() < 2 {
        return PlaybackStep::Stay;
    }
    let start = full
        .iter()
        .position(|offset| *offset == shown_offset)
        .unwrap_or(0);
    for step in 1..full.len() {
        let offset = full[(start + step) % full.len()];
        if ready.contains(&offset) {
            return PlaybackStep::Advance(offset);
        }
        if pending.contains(&offset) {
            return PlaybackStep::Wait;
        }
    }
    PlaybackStep::Stay
}

#[cfg(test)]
mod tests {
    use coldfront_core::{RadarSpeed, applet_radar_interval_ms};

    use super::{
        GESTURE_FRAME, GESTURE_PIXEL_BUDGET, PlaybackStep, format_panel_temperature,
        gesture_paint_due, gesture_render_scale, merge_lookback_offsets, next_playback_step,
        playback_advance_during_gesture, prefetch_absorb_allowed,
    };

    #[test]
    fn panel_temperature_includes_unit() {
        assert_eq!(format_panel_temperature(11.666_666_666_7, true), "53°F");
        assert_eq!(format_panel_temperature(12.4, false), "12°C");
        assert_eq!(format_panel_temperature(-1.5, false), "-2°C");
    }

    #[test]
    fn gesture_paint_waits_for_the_frame_budget() {
        let now = std::time::Instant::now();
        assert!(gesture_paint_due(None, now));
        assert!(!gesture_paint_due(Some(now), now + GESTURE_FRAME / 2));
        assert!(gesture_paint_due(Some(now), now + GESTURE_FRAME));
    }

    #[test]
    fn gesture_frames_stay_inside_the_pixel_budget() {
        assert!((gesture_render_scale(416, 220) - 1.0).abs() < 1e-9);
        let fullscreen = gesture_render_scale(1872, 880);
        assert!(fullscreen < 1.0);
        let painted = f64::from(1872) * f64::from(880) * fullscreen * fullscreen;
        assert!((painted - GESTURE_PIXEL_BUDGET).abs() < 1.0);
        assert!((gesture_render_scale(8000, 4000) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn prefetch_ignores_stale_generation() {
        assert!(prefetch_absorb_allowed(2, 2));
        assert!(!prefetch_absorb_allowed(1, 2));
    }

    #[test]
    fn playback_defers_offset_while_gesturing() {
        let (shown, deferred) = playback_advance_during_gesture(true, 50, 40);
        assert_eq!(shown, 50);
        assert_eq!(deferred, Some(40));
        let (shown, deferred) = playback_advance_during_gesture(false, 50, 40);
        assert_eq!(shown, 40);
        assert_eq!(deferred, None);
    }

    #[test]
    fn radar_interval_scales_with_speed() {
        assert_eq!(applet_radar_interval_ms(RadarSpeed::X1), 800);
        assert_eq!(applet_radar_interval_ms(RadarSpeed::X2), 400);
        assert_eq!(applet_radar_interval_ms(RadarSpeed::X4), 200);
    }

    #[test]
    fn lookback_grow_queues_missing_behind_pending() {
        let allowed = [120, 110, 100, 90, 80, 70, 60, 50, 40, 30, 20, 10, 0];
        let pending = vec![10, 20, 30, 40, 50];
        let ready = vec![0];
        let (next_pending, next_ready, shown) =
            merge_lookback_offsets(&allowed, &pending, &ready, 0);
        assert_eq!(shown, 0);
        assert_eq!(next_ready, vec![0]);
        assert_eq!(next_pending.last().copied(), Some(50));
        assert!(next_pending.starts_with(&[120, 110, 100, 90, 80, 70, 60]));
        assert!(!next_pending.contains(&0));
    }

    #[test]
    fn lookback_shrink_drops_outside_offsets() {
        let allowed = [50, 40, 30, 20, 10, 0];
        let pending = vec![120, 60, 10];
        let ready = vec![0, 50, 180];
        let (next_pending, next_ready, shown) =
            merge_lookback_offsets(&allowed, &pending, &ready, 180);
        assert_eq!(shown, 0);
        assert_eq!(next_ready, vec![0, 50]);
        assert!(!next_pending.contains(&120));
        assert!(!next_pending.contains(&60));
        assert!(!next_pending.contains(&180));
        assert!(!next_pending.contains(&0));
    }

    #[test]
    fn playback_waits_on_pending_next_offset() {
        let full = [50, 40, 30, 20, 10, 0];
        let ready = std::collections::HashSet::from([0, 50]);
        let pending = std::collections::HashSet::from([40]);
        assert_eq!(
            next_playback_step(&full, &ready, &pending, 50),
            PlaybackStep::Wait
        );
    }

    #[test]
    fn playback_skips_failed_offset_to_next_ready() {
        let full = [50, 40, 30, 20, 10, 0];
        let ready = std::collections::HashSet::from([0, 50, 30]);
        let pending = std::collections::HashSet::new();
        assert_eq!(
            next_playback_step(&full, &ready, &pending, 50),
            PlaybackStep::Advance(30)
        );
    }

    #[test]
    fn playback_advances_when_next_is_ready() {
        let full = [50, 40, 30, 20, 10, 0];
        let ready = std::collections::HashSet::from([0, 50, 40]);
        let pending = std::collections::HashSet::new();
        assert_eq!(
            next_playback_step(&full, &ready, &pending, 50),
            PlaybackStep::Advance(40)
        );
    }
}
