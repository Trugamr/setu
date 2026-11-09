//! The window. It hosts the app's [`Session`]: the fields send it commands, the control port
//! lets `setu-cli` send it more, and a poll ten times a second answers those and renders the
//! session's [`Snapshot`].

use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{IndexPath, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use setu::control;
use setu::output;
use setu::settings::Store;
use setu::view::{Look, Summary};
use setu::{Command, Session, Snapshot};

use crate::brand::{self, hsla};
use crate::dimens::{self, Type};
use crate::palette::*;

/// The device list's first entry, which follows the system's default output.
const SYSTEM_DEFAULT: &str = "System default";
const POLL: Duration = Duration::from_millis(100);

pub fn run() {
    // The component icons, such as the select's chevron.
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            brand::apply(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(440.), px(540.)),
                    cx,
                ))),
                window_min_size: Some(size(px(380.), px(500.))),
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

    /// The status card: what is happening, and the one action for it. Not connected, it holds
    /// the address field and Connect; connected, Disconnect.
    fn status_card(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let snapshot = &self.snapshot;
        let summary = &snapshot.summary;
        let look = summary.phase.look();
        let flowing = look == Look::Flowing;
        let secondary = hsla(if flowing { LINE } else { CREAM_MUTED });
        let sentence = match look {
            _ if summary.phase.is_problem() => hsla(SIGNAL),
            Look::Waiting => hsla(CREAM),
            _ => secondary,
        };
        let headset = snapshot.settings.headset.clone();

        let toggle = cx.listener(|this, _: &ClickEvent, window, cx| this.toggle(window, cx));
        let action = if !snapshot.connected {
            h_flex()
                .mt(px(dimens::GAP))
                .gap(px(8.))
                .child(div().flex_1().child(Input::new(&self.headset).large()))
                .child(
                    Button::new("connect")
                        .label("Connect")
                        .primary()
                        .large()
                        .px(px(20.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .on_click(toggle),
                )
                .into_any_element()
        } else {
            let button = Button::new("disconnect")
                .large()
                .w_full()
                .font_weight(FontWeight::SEMIBOLD)
                .on_click(toggle);
            let button = match look {
                // Ink on the Cream card.
                Look::Flowing => button.label("Disconnect").custom(
                    ButtonCustomVariant::new(cx)
                        .color(hsla(INK))
                        .foreground(hsla(CREAM))
                        .hover(hsla(INK_RAISED))
                        .active(hsla(INK_HIGH)),
                ),
                // Outlined in Edge with no fill: the kit's outline style fills with Edge too.
                _ => button
                    .label(if headset.is_empty() {
                        "Disconnect".to_string()
                    } else {
                        format!("Disconnect from {headset}")
                    })
                    .h(px(38.))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(hsla(INK_RAISED))
                            .foreground(hsla(CREAM))
                            .hover(hsla(INK_HIGH))
                            .active(hsla(INK_HOVER)),
                    ),
            };
            let button = if look == Look::Flowing {
                div().child(button)
            } else {
                div()
                    .rounded(px(dimens::CONTROL_RADIUS))
                    .border_1()
                    .border_color(hsla(EDGE))
                    .child(button)
            };
            div().mt(px(dimens::GAP)).child(button).into_any_element()
        };

        v_flex()
            .p(px(dimens::CARD_PADDING))
            .rounded(px(dimens::CARD_RADIUS))
            .bg(hsla(if flowing { CREAM } else { INK_RAISED }))
            .text_color(hsla(if flowing { INK } else { CREAM }))
            .when(look == Look::Waiting, |card| {
                card.border_2().border_color(hsla(CREAM))
            })
            .child(
                h_flex()
                    .items_start()
                    .gap(px(dimens::GAP))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(dimens::TIGHT))
                            .child(
                                text(&dimens::TITLE)
                                    .font_weight(FontWeight::BOLD)
                                    .child(summary.title.clone()),
                            )
                            .child(
                                text(&dimens::SENTENCE)
                                    .text_color(sentence)
                                    .child(summary.detail.clone()),
                            ),
                    )
                    .child(brand::grille(look, summary.level)),
            )
            .when(flowing, |card| {
                card.child(facts(summary, &headset, secondary))
            })
            .child(action)
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
        let exclusive = self.snapshot.settings.exclusive;
        let footer = self.snapshot.summary.footer.clone();

        v_flex()
            .size_full()
            .bg(hsla(INK))
            .text_color(hsla(CREAM))
            .p(px(dimens::MARGIN))
            .child(self.status_card(cx))
            .child(section_label("Output"))
            .child(
                v_flex()
                    .rounded(px(dimens::CARD_RADIUS))
                    .bg(hsla(INK_RAISED))
                    .child(
                        row()
                            .child(text(&dimens::ROW).flex_1().child("Device"))
                            .child(
                                div().w(px(220.)).child(
                                    Select::new(&self.device).menu_width(px(280.)),
                                ),
                            ),
                    )
                    .child(div().h(px(1.)).bg(hsla(INK_HIGH)))
                    .child(
                        row()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(text(&dimens::ROW).child("Exclusive mode"))
                                    .child(text(&dimens::LABEL).text_color(hsla(CREAM_MUTED)).child(
                                        "Lowest latency. Other apps can't use the device meanwhile.",
                                    )),
                            )
                            .child(switch("exclusive", exclusive).on_click(cx.listener(
                                move |this, _: &ClickEvent, window, cx| {
                                    this.apply(Command::Exclusive(!exclusive), window, cx);
                                },
                            ))),
                    ),
            )
            .child(div().flex_1())
            .when_some(footer, |page, footer| {
                page.child(
                    text(&dimens::FACT_LABEL)
                        .px(px(dimens::CARD_PADDING))
                        .text_color(hsla(CREAM_MUTED))
                        .child(footer),
                )
            })
    }
}

/// While playing: where from, the buffer and the underruns.
fn facts(summary: &Summary, headset: &str, secondary: Hsla) -> impl IntoElement {
    let fact = |value: String, label: &'static str| {
        v_flex()
            .child(
                text(&dimens::FACT)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value),
            )
            .child(text(&dimens::FACT_LABEL).text_color(secondary).child(label))
    };
    h_flex()
        .mt(px(dimens::GAP))
        .gap(px(24.))
        .when(!headset.is_empty(), |row| {
            row.child(fact(headset.to_string(), "headset"))
        })
        .children(summary.facts.iter().map(|f| fact(f.value.clone(), f.label)))
}

fn section_label(label: &'static str) -> impl IntoElement {
    text(&dimens::LABEL)
        .text_color(hsla(CREAM_MUTED))
        .pt(px(dimens::SECTION_GAP))
        .pb(px(8.))
        .px(px(dimens::CARD_PADDING))
        .child(label)
}

fn row() -> Div {
    h_flex()
        .items_center()
        .gap(px(12.))
        .min_h(px(dimens::ROW_HEIGHT))
        .px(px(dimens::CARD_PADDING))
        .py(px(10.))
}

/// A switch in the brand's colours, which the component theme can't give: on, a Cream track
/// with an Ink thumb; off, an Edge outline and thumb.
fn switch(id: &'static str, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .w(px(40.))
        .h(px(20.))
        .rounded_full()
        .p(px(4.))
        .flex()
        .cursor_pointer()
        .when(on, |track| track.bg(hsla(CREAM)).justify_end())
        .when(!on, |track| {
            track.border_1().border_color(hsla(EDGE)).p(px(3.))
        })
        .child(
            div()
                .size(px(12.))
                .rounded_full()
                .bg(hsla(if on { INK } else { EDGE })),
        )
}

fn text(style: &Type) -> Div {
    div().text_size(px(style.0)).line_height(px(style.1))
}
