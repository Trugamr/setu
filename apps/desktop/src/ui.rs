//! The window. It hosts the app's [`Session`]: the fields send it commands, the control port
//! lets `setu-cli` send it more, and a poll ten times a second answers those and renders the
//! session's [`Snapshot`].

use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{IndexPath, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use setu::control;
use setu::output;
use setu::settings::Store;
use setu::view::{Phase, Summary};
use setu::{Command, Session, Snapshot};

use crate::brand;

/// The device list's first entry, which follows the system's default output.
const SYSTEM_DEFAULT: &str = "System default";
const POLL: Duration = Duration::from_millis(100);

pub fn run() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        brand::apply(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(440.), px(640.)),
                cx,
            ))),
            window_min_size: Some(size(px(380.), px(580.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Setu".into()),
                ..Default::default()
            }),
            app_id: Some("dev.trugamr.setu".into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Setu::new(window, cx)))
            .expect("could not open the window");
        // Closing the window quits; the receiver stops when it drops.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}

struct Setu {
    session: Session,
    /// `None` when another Setu holds the control port.
    control: Option<control::Server>,
    headset: Entity<InputState>,
    /// [`SYSTEM_DEFAULT`], then the output devices.
    device_names: Vec<SharedString>,
    device: Entity<SelectState<Vec<SharedString>>>,
    snapshot: Snapshot,
    _subscriptions: Vec<Subscription>,
}

impl Setu {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = Store::user();
        let settings = store.as_ref().map(Store::load).unwrap_or_default();
        let session = Session::new(settings, store);
        let control = control::Server::bind()
            .inspect_err(|e| eprintln!("not taking control requests: {e}"))
            .ok();
        let snapshot = session.snapshot();

        let headset = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Headset address, e.g. 192.168.1.42")
                .default_value(snapshot.settings.headset.clone())
        });

        let mut device_names: Vec<SharedString> = vec![SYSTEM_DEFAULT.into()];
        if let Ok(devices) = output::devices() {
            device_names.extend(devices.names.into_iter().map(SharedString::from));
        }
        let selected = device_index(&device_names, snapshot.settings.device.as_deref());
        let device = cx.new(|cx| {
            SelectState::new(
                device_names.clone(),
                Some(IndexPath::new(selected)),
                window,
                cx,
            )
        });

        let subscriptions = vec![
            cx.subscribe_in(
                &headset,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.toggle(window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &device,
                window,
                |this, _, event: &SelectEvent<Vec<SharedString>>, window, cx| {
                    let SelectEvent::Confirm(value) = event;
                    let device = value
                        .as_ref()
                        .filter(|name| name.as_ref() != SYSTEM_DEFAULT)
                        .map(|name| name.to_string());
                    this.apply(Command::Device(device), window, cx);
                },
            ),
        ];

        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let tick = this.update_in(cx, |this, window, cx| {
                    if let Some(control) = &this.control {
                        control.answer(&mut this.session);
                    }
                    this.refresh(window, cx);
                });
                if tick.is_err() {
                    break;
                }
            }
        })
        .detach();

        Setu {
            session,
            control,
            headset,
            device_names,
            device,
            snapshot,
            _subscriptions: subscriptions,
        }
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = if self.snapshot.connected {
            Command::Disconnect
        } else {
            let typed = self.headset.read(cx).value().to_string();
            Command::Connect(Some(typed))
        };
        self.apply(command, window, cx);
    }

    /// Applies a command from the window's own fields. A failure shows in the snapshot.
    fn apply(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.session.apply(command);
        self.refresh(window, cx);
    }

    /// Re-renders when the session changed, and brings a field up to date when its setting
    /// changed elsewhere, such as through a control request. A field whose setting is unchanged
    /// keeps whatever is being typed into it.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.session.snapshot();
        if snapshot == self.snapshot {
            return;
        }
        let (old, new) = (&self.snapshot.settings, &snapshot.settings);
        if new.headset != old.headset && self.headset.read(cx).value().trim() != new.headset {
            let headset = new.headset.clone();
            self.headset
                .update(cx, |input, cx| input.set_value(headset, window, cx));
        }
        let selected = IndexPath::new(device_index(&self.device_names, new.device.as_deref()));
        if new.device != old.device && self.device.read(cx).selected_index(cx) != Some(selected) {
            self.device.update(cx, |select, cx| {
                select.set_selected_index(Some(selected), window, cx)
            });
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

/// The list entry for a device setting: the exact name, else the first name containing it
/// (ignoring case, as the output matches it), else the system default.
fn device_index(names: &[SharedString], device: Option<&str>) -> usize {
    let Some(device) = device else {
        return 0;
    };
    let wanted = device.to_lowercase();
    names
        .iter()
        .position(|name| name.as_ref() == device)
        .or_else(|| {
            names
                .iter()
                .skip(1)
                .position(|name| name.to_lowercase().contains(&wanted))
                .map(|i| i + 1)
        })
        .unwrap_or(0)
}

impl Render for Setu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = &self.snapshot;
        let connected = snapshot.connected;
        let summary = &snapshot.summary;

        let connect = Button::new("connect")
            .label(if connected { "Disconnect" } else { "Connect" })
            .map(|button| {
                if connected {
                    button.outline()
                } else {
                    button.primary()
                }
            })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle(window, cx)));

        let exclusive = Switch::new("exclusive")
            .checked(snapshot.settings.exclusive)
            .on_click(cx.listener(|this, checked: &bool, window, cx| {
                this.apply(Command::Exclusive(*checked), window, cx);
            }));

        v_flex()
            .size_full()
            .bg(brand::ink())
            .text_color(brand::cream())
            .px(px(28.))
            .py(px(24.))
            .gap(px(22.))
            .child(header(summary.phase))
            .child(status_card(summary))
            .child(field(
                "Headset",
                h_flex()
                    .gap(px(8.))
                    .child(div().flex_1().child(Input::new(&self.headset)))
                    .child(connect),
            ))
            .child(field(
                "Output",
                v_flex()
                    .gap(px(14.))
                    .child(Select::new(&self.device))
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap(px(16.))
                            .child(v_flex().flex_1().min_w_0().child("Exclusive mode").child(
                                muted(
                                    "Lowest latency. Other apps cannot use the device meanwhile.",
                                ),
                            ))
                            .child(div().flex_none().child(exclusive)),
                    ),
            ))
            .when_some(snapshot.error.clone(), |el, error| {
                el.child(div().text_sm().text_color(brand::signal()).child(error))
            })
            .child(div().flex_1())
            .child(
                div().text_xs().text_color(brand::cream_muted()).child(
                    summary
                        .stats
                        .clone()
                        .unwrap_or_else(|| "Not streaming".into()),
                ),
            )
    }
}

fn header(phase: Phase) -> impl IntoElement {
    let (label, live) = match phase {
        Phase::Stopped => ("Off", false),
        Phase::Waiting | Phase::Buffering => ("Waiting", false),
        Phase::Playing => ("Live", true),
        Phase::Unsupported | Phase::OutputLost => ("Problem", false),
    };
    h_flex()
        .justify_between()
        .items_center()
        .child(
            h_flex()
                .gap(px(12.))
                .items_center()
                .child(brand::mark(36.))
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Setu"),
                ),
        )
        .child(
            h_flex()
                .gap(px(6.))
                .items_center()
                .px(px(10.))
                .py(px(4.))
                .rounded_full()
                .border_1()
                .border_color(if live { brand::signal() } else { brand::line() })
                .child(div().size(px(7.)).rounded_full().bg(if live {
                    brand::signal()
                } else {
                    brand::cream_muted()
                }))
                .child(div().text_xs().child(label)),
        )
}

fn status_card(summary: &Summary) -> impl IntoElement {
    v_flex()
        .gap(px(8.))
        .p(px(20.))
        .rounded(px(16.))
        .bg(brand::ink_raised())
        .border_1()
        .border_color(brand::ink_high())
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(summary.title.clone()),
        )
        .child(muted(summary.detail.clone()))
        .child(div().pt(px(8.)).child(brand::meter(summary.level, 16)))
}

fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap(px(8.))
        .child(
            div()
                .text_sm()
                .text_color(brand::cream_muted())
                .child(label),
        )
        .child(control)
}

fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .text_color(brand::cream_muted())
        .child(text.into())
}
