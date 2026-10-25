use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_plot::{Legend, Line, Plot, PlotPoint, PlotPoints, PlotResponse, Points};

use crate::client::{NetworkSimulation, Owned};
use crate::diagnostics::{MAX_WINDOW_SECONDS, MessageContent, MessageEvent, NetworkDiagnostics};
use crate::rendering::InterpolationDebug;
use crate::state::RemoteWorld;
use shared::{
    components::{Health, Position},
    snapshots::NetworkIdentity,
};

const KILOBITS_PER_BYTE: f64 = 8.0 / 1_000.0;
const PACKET_DOT_RADIUS: f32 = 5.0;
const PACKET_CLICK_RADIUS: f32 = 9.0;
const MIN_WINDOW_SECONDS: f64 = 0.05;
const DEFAULT_WINDOW_SECONDS: f64 = 10.0;

pub struct TimelineZoom {
    span_s: f64,
}

impl Default for TimelineZoom {
    fn default() -> Self {
        Self {
            span_s: DEFAULT_WINDOW_SECONDS,
        }
    }
}

impl TimelineZoom {
    fn set_span(&mut self, span_s: f64) {
        self.span_s = span_s.clamp(MIN_WINDOW_SECONDS, MAX_WINDOW_SECONDS);
    }
}

pub fn network_diagnostics_ui(
    mut contexts: EguiContexts,
    mut stats: ResMut<NetworkDiagnostics>,
    mut simulation: ResMut<NetworkSimulation>,
    remote_world: Res<RemoteWorld>,
    interpolation_debug: Query<&InterpolationDebug>,
    entities: Query<(
        Entity,
        &NetworkIdentity,
        &Position,
        &Transform,
        Option<&Health>,
        Option<&Owned>,
    )>,
    mut selected_packet: Local<Option<MessageEvent>>,
    mut zoom: Local<TimelineZoom>,
) -> Result {
    let context = contexts.ctx_mut()?;
    egui::Window::new("Network diagnostics").show(context, |ui| {
        ui.label(format!("Ping/pong RTT: {} ms", stats.ping_ms));
        if ui
            .button(if stats.capture_paused {
                "Resume capture"
            } else {
                "Pause capture"
            })
            .clicked()
        {
            stats.capture_paused = !stats.capture_paused;
        }
        ui.horizontal(|ui| {
            ui.label("Client outgoing delay");
            ui.add(egui::Slider::new(&mut simulation.outgoing_delay_ms, 0..=1_000).suffix(" ms"));
        });
        ui.horizontal(|ui| {
            ui.label("Incoming server delay");
            ui.add(egui::Slider::new(&mut simulation.incoming_delay_ms, 0..=1_000).suffix(" ms"));
        });
        ui.small("Delay changes apply to new messages. RTT starts when a ping is sent and includes incoming Pong delay.");
        ui.collapsing("Replicated entities", |ui| {
            let mut rows: Vec<_> = entities.iter().collect();
            rows.sort_by_key(|(_, identity, _, _, _, _)| identity.id);
            for (_, identity, position, _, health, owned) in rows {
                let health = health.map_or_else(|| "none".to_owned(), |value| value.0.to_string());
                let ownership = if owned.is_some() { "owned locally" } else { "remote" };
                ui.monospace(format!(
                    "#{} | server position ({:.1}, {:.1}) | health {} | {}",
                    identity.id, position.x, position.y, health, ownership,
                ));
            }
        });
        ui.collapsing("Interpolation", |ui| {
            ui.small("Latest = last received position. Rendered = circle on screen. The gap is expected while interpolation plays buffered movement.");
            ui.small("This view is live, including while packet capture is paused.");
            let mut rows: Vec<_> = entities.iter().collect();
            rows.sort_by_key(|(_, identity, _, _, _, _)| identity.id);
            for (entity, identity, position, transform, _, owned) in rows {
                ui.push_id(identity.id, |ui| {
                    show_circle_interpolation(ui, identity, position, transform, owned, &remote_world);
                    if owned.is_none() {
                        if let Ok(debug) = interpolation_debug.get(entity) {
                            show_interpolation_inspector(ui, debug);
                        }
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            ui.label("Time window");
            ui.add(
                egui::Slider::new(&mut zoom.span_s, MIN_WINDOW_SECONDS..=MAX_WINDOW_SECONDS)
                    .suffix(" s"),
            );
        });
        ui.small("Scroll over either graph to zoom");
        let now = graph_now(&stats);
        let span_s = zoom.span_s;
        let rate_hovered = show_network_graph(ui, &stats, now, span_s);
        let packets_hovered = show_packet_timeline(ui, &stats, now, span_s, &mut selected_packet);
        if rate_hovered || packets_hovered {
            let scroll_y = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll_y != 0.0 {
                zoom.set_span(span_s * (-(scroll_y as f64) / 250.0).exp());
                ui.input_mut(|input| input.smooth_scroll_delta.y = 0.0);
            }
        }
        ui.separator();
        show_packet_list(ui, &stats, now, span_s, &mut selected_packet);
    });
    show_packet_inspector(context, &stats, &mut selected_packet);
    Ok(())
}

fn show_circle_interpolation(
    ui: &mut egui::Ui,
    identity: &NetworkIdentity,
    position: &Position,
    transform: &Transform,
    owned: Option<&Owned>,
    remote_world: &RemoteWorld,
) {
    ui.separator();
    ui.label(format!("Circle #{}", identity.id));

    if owned.is_some() {
        ui.small("Locally owned: interpolation bypassed.");
        return;
    }

    let latest = Vec2::new(position.x, position.y);
    let rendered = transform.translation.truncate();
    ui.monospace(format!(
        "Latest:   ({:.2}, {:.2})\nRendered: ({:.2}, {:.2})\nGap:      {:.2} world units",
        latest.x,
        latest.y,
        rendered.x,
        rendered.y,
        latest.distance(rendered),
    ));

    let Some(samples) = remote_world.positions.get(&identity.id) else {
        ui.small("No interpolation samples buffered.");
        return;
    };

    ui.label(format!("Buffered samples: {}", samples.len()));
    if let (Some(first), Some(last)) = (samples.front(), samples.back()) {
        ui.monospace(format!("Buffered ticks: {} → {}", first.tick, last.tick));
    }

    let path: PlotPoints = samples
        .iter()
        .map(|sample| [sample.x as f64, sample.y as f64])
        .collect();

    Plot::new(("circle_interpolation", identity.id))
        .height(180.0)
        .data_aspect(1.0)
        .x_axis_label("World X")
        .y_axis_label("World Y")
        .legend(Legend::default())
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new("Buffered path", path).color(egui::Color32::GRAY));
            plot_ui.points(
                Points::new("Latest received", vec![[latest.x as f64, latest.y as f64]])
                    .color(egui::Color32::from_rgb(255, 170, 60))
                    .radius(7.0),
            );
            plot_ui.points(
                Points::new("Rendered", vec![[rendered.x as f64, rendered.y as f64]])
                    .color(egui::Color32::from_rgb(80, 200, 255))
                    .radius(5.0),
            );
        });
}

fn show_interpolation_inspector(ui: &mut egui::Ui, debug: &InterpolationDebug) {
    ui.collapsing("Sample inspector", |ui| {
        ui.monospace(format!("Display tick: {:.3}", debug.display_tick));
        let Some(sample) = debug.sample else {
            ui.label("No interpolation used on the last rendered frame.");
            return;
        };
        ui.label(sample.status);
        ui.monospace(format!(
            "From tick {}: ({:.2}, {:.2})\nTo tick   {}: ({:.2}, {:.2})",
            sample.from.tick, sample.from.x, sample.from.y,
            sample.to.tick, sample.to.x, sample.to.y,
        ));
        if sample.from.tick != sample.to.tick {
            ui.add(egui::ProgressBar::new(sample.fraction).text(format!(
                "Blend: {:.3} ({:.1}%)", sample.fraction, sample.fraction * 100.0,
            )));
        } else {
            ui.small("One sample held; no blend between two samples.");
        }
        ui.monospace(format!("Result: ({:.2}, {:.2})", sample.position.x, sample.position.y));
        ui.small("Captured from rendering. Ticks identify position samples, not packet sequence numbers.");
    });
}

fn graph_now(stats: &NetworkDiagnostics) -> f64 {
    stats
        .samples
        .back()
        .map_or(0.0, |sample| sample.time_s)
        .max(
            stats
                .message_events
                .back()
                .map_or(0.0, |event| event.time_s),
        )
}

fn show_network_graph(
    ui: &mut egui::Ui,
    stats: &NetworkDiagnostics,
    now: f64,
    span_s: f64,
) -> bool {
    let max_rate = stats
        .samples
        .iter()
        .filter(|sample| in_window(sample.time_s, now, span_s))
        .map(|sample| sample.sent_bytes_per_s.max(sample.received_bytes_per_s))
        .fold(0.0_f64, f64::max);
    let y_max = 16.0_f64.max(max_rate * KILOBITS_PER_BYTE * 1.2);

    let sent: PlotPoints = stats
        .samples
        .iter()
        .filter(|sample| in_window(sample.time_s, now, span_s))
        .map(|sample| {
            [
                sample.time_s - now,
                sample.sent_bytes_per_s * KILOBITS_PER_BYTE,
            ]
        })
        .collect();
    let received: PlotPoints = stats
        .samples
        .iter()
        .filter(|sample| in_window(sample.time_s, now, span_s))
        .map(|sample| {
            [
                sample.time_s - now,
                sample.received_bytes_per_s * KILOBITS_PER_BYTE,
            ]
        })
        .collect();

    let response = Plot::new("network_speed")
        .width(500.0)
        .height(200.0)
        .default_x_bounds(-span_s, 0.0)
        .default_y_bounds(0.0, 16.0)
        .x_axis_label("Seconds ago")
        .y_axis_label("kb/s")
        .legend(Legend::default())
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .show(ui, |plot_ui| {
            plot_ui.set_plot_bounds_x(-span_s..=0.0);
            plot_ui.set_plot_bounds_y(0.0..=y_max);
            plot_ui.line(Line::new("Sent kb/s", sent));
            plot_ui.line(Line::new("Received kb/s", received));
        });
    response.response.hovered()
}

fn show_packet_timeline(
    ui: &mut egui::Ui,
    stats: &NetworkDiagnostics,
    now: f64,
    span_s: f64,
    selected_packet: &mut Option<MessageEvent>,
) -> bool {
    ui.heading("Packet timeline");
    let sent_color = egui::Color32::from_rgb(80, 170, 255);
    let received_color = egui::Color32::from_rgb(100, 220, 140);
    ui.horizontal(|ui| {
        ui.colored_label(sent_color, "● Sent");
        ui.colored_label(received_color, "● Received");
        ui.label("Click a dot to inspect it");
    });

    let sent: PlotPoints = stats
        .message_events
        .iter()
        .filter(|event| {
            in_window(event.time_s, now, span_s) && matches!(event.content, MessageContent::Sent(_))
        })
        .map(|event| [event.time_s - now, packet_lane(event)])
        .collect();
    let received: PlotPoints = stats
        .message_events
        .iter()
        .filter(|event| {
            in_window(event.time_s, now, span_s)
                && matches!(event.content, MessageContent::Received(_))
        })
        .map(|event| [event.time_s - now, packet_lane(event)])
        .collect();

    let response = Plot::new("packet_timeline")
        .width(500.0)
        .height(130.0)
        .default_x_bounds(-span_s, 0.0)
        .default_y_bounds(-0.5, 1.5)
        .x_axis_label("Seconds ago")
        .show_y(false)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .sense(egui::Sense::click())
        .show(ui, |plot_ui| {
            plot_ui.set_plot_bounds_x(-span_s..=0.0);
            plot_ui.set_plot_bounds_y(-0.5..=1.5);
            plot_ui.points(
                Points::new("Sent packets", sent)
                    .color(sent_color)
                    .radius(PACKET_DOT_RADIUS),
            );
            plot_ui.points(
                Points::new("Received packets", received)
                    .color(received_color)
                    .radius(PACKET_DOT_RADIUS),
            );
        });

    if let Some(packet) = clicked_packet(&response, stats, now, span_s) {
        *selected_packet = Some(packet.clone());
    }
    response.response.hovered()
}

fn packet_lane(event: &MessageEvent) -> f64 {
    match event.content {
        MessageContent::Sent(_) => 1.0,
        MessageContent::Received(_) => 0.0,
    }
}

fn clicked_packet<'a>(
    response: &PlotResponse<()>,
    stats: &'a NetworkDiagnostics,
    now: f64,
    span_s: f64,
) -> Option<&'a MessageEvent> {
    if !response.response.clicked() {
        return None;
    }
    let pointer = response.response.interact_pointer_pos()?;
    nearest_packet(stats.message_events.iter(), now, span_s, pointer, |point| {
        response.transform.position_from_point(&point)
    })
}

fn nearest_packet<'a>(
    events: impl Iterator<Item = &'a MessageEvent>,
    now: f64,
    span_s: f64,
    pointer: egui::Pos2,
    position_of: impl Fn(PlotPoint) -> egui::Pos2,
) -> Option<&'a MessageEvent> {
    events
        .filter_map(|event| {
            let x = event.time_s - now;
            if !(-span_s..=0.0).contains(&x) {
                return None;
            }
            let position = position_of(PlotPoint::new(x, packet_lane(event)));
            let distance_sq = pointer.distance_sq(position);
            (distance_sq <= PACKET_CLICK_RADIUS.powi(2)).then_some((distance_sq, event))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, event)| event)
}

fn in_window(time_s: f64, now: f64, span_s: f64) -> bool {
    (now - span_s..=now).contains(&time_s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::client_messages::{ClientMessage, ClientMessages};
    use shared::server_messages::{ServerMessage, ServerMessages};

    #[test]
    fn selects_the_nearest_visible_dot() {
        let events = [
            MessageEvent {
                id: 1,
                time_s: 9.0,
                rtt_ms: None,
                content: MessageContent::Sent(ClientMessage {
                    sequence: 1,
                    payload: ClientMessages::Ping {
                        text: "ping".into(),
                    },
                }),
            },
            MessageEvent {
                id: 2,
                time_s: 9.1,
                rtt_ms: None,
                content: MessageContent::Received(ServerMessage {
                    sequence: 1,
                    tick: 1,
                    payload: ServerMessages::Pong("pong".into()),
                }),
            },
        ];
        let position_of =
            |point: PlotPoint| egui::pos2((point.x * 10.0) as f32, (point.y * 20.0) as f32);

        let received = nearest_packet(events.iter(), 10.0, 2.0, egui::pos2(-9.0, 0.0), position_of);
        assert_eq!(received.map(|event| event.id), Some(2));

        let empty = nearest_packet(
            events.iter(),
            10.0,
            2.0,
            egui::pos2(100.0, 100.0),
            position_of,
        );
        assert!(empty.is_none());

        let zoomed_in =
            nearest_packet(events.iter(), 10.0, 0.5, egui::pos2(-9.0, 0.0), position_of);
        assert!(zoomed_in.is_none());
    }
}

fn show_packet_list(
    ui: &mut egui::Ui,
    stats: &NetworkDiagnostics,
    now: f64,
    span_s: f64,
    selected_packet: &mut Option<MessageEvent>,
) {
    ui.heading("Packets");
    ui.monospace("Time       Direction    Sequence   Type          RTT");
    let visible_events = || {
        stats
            .message_events
            .iter()
            .rev()
            .filter(|event| in_window(event.time_s, now, span_s))
    };
    let row_count = visible_events().count();
    let row_height = ui.spacing().interact_size.y;
    egui::ScrollArea::vertical()
        .id_salt("packet_list")
        .max_height(250.0)
        .show_rows(ui, row_height, row_count, |ui, rows| {
            for event in visible_events().skip(rows.start).take(rows.len()) {
                let label = format!(
                    "{:>7.3} s  {:<8}  #{:<7}  {:<14}  {}",
                    event.time_s,
                    event.direction(),
                    event.sequence(),
                    event.payload_type(),
                    rtt_label(event)
                );
                let selected = selected_packet
                    .as_ref()
                    .is_some_and(|packet| packet.id == event.id);
                if ui
                    .selectable_label(selected, egui::RichText::new(label).monospace())
                    .clicked()
                {
                    *selected_packet = Some(event.clone());
                }
            }
        });
}

fn show_packet_inspector(
    context: &egui::Context,
    stats: &NetworkDiagnostics,
    selected_packet: &mut Option<MessageEvent>,
) {
    let Some(packet) = selected_packet.as_mut() else {
        return;
    };
    if let Some(updated) = stats
        .message_events
        .iter()
        .find(|event| event.id == packet.id)
    {
        *packet = updated.clone();
    }

    let mut open = true;
    egui::Window::new("Packet inspector")
        .open(&mut open)
        .default_width(520.0)
        .resizable(true)
        .show(context, |ui| {
            egui::Grid::new("packet_fields").show(ui, |ui| {
                ui.label("Direction");
                ui.label(packet.direction());
                ui.end_row();

                ui.label("Sequence");
                ui.label(packet.sequence().to_string());
                ui.end_row();

                ui.label("Time");
                ui.label(format!("{:.3} s since startup", packet.time_s));
                ui.end_row();

                ui.label("RTT");
                ui.label(rtt_label(packet));
                ui.end_row();

                ui.label("Payload type");
                ui.label(packet.payload_type());
                ui.end_row();
            });
            ui.separator();
            ui.label("Decoded payload");
            egui::ScrollArea::both()
                .id_salt("decoded_payload")
                .max_height(350.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(packet.decoded_payload()).monospace())
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .selectable(true),
                    );
                });
        });
    if !open {
        *selected_packet = None;
    }
}

fn rtt_label(packet: &MessageEvent) -> String {
    match packet.rtt_ms {
        Some(ms) => format!("{ms:.2} ms"),
        None if matches!(packet.content, MessageContent::Sent(_)) => "Pending".into(),
        None => "—".into(),
    }
}
