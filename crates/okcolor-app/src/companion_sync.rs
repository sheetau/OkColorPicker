//! UI-thread reconciliation between precise local edits and CSP observations.
use crate::model::{ColorEdit, EditOrigin, PickerModel};
use okcolor_core::{Gamut, GamutMapping};
use okcolor_csp::{
    color::{ColorCommand, ColorState},
    worker::{Event, State},
};
use std::time::{Duration, Instant};
use zgui::prelude::*;

#[derive(Clone, Copy)]
pub(crate) struct Observation {
    pub color: ColorState,
    pub after_write: u64,
    pub sequence: u64,
}
#[derive(Clone, Default)]
pub(crate) struct Inbox {
    pub generation: u64,
    pub state: Option<State>,
    pub requested: bool,
    pub color: Option<Observation>,
    pub rejected: Option<u64>,
    sequence: u64,
}
impl Inbox {
    pub fn absorb(&mut self, generation: u64, event: Event) -> bool {
        if generation < self.generation || matches!(event, Event::Push(_) | Event::Sent { .. }) {
            return false;
        }
        if generation != self.generation {
            *self = Self {
                generation,
                ..Default::default()
            };
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("CSP observation sequence exhausted");
        match event {
            Event::State(state) => {
                self.requested |= matches!(state, State::Scanning | State::Connecting);
                if state != State::Connected {
                    self.color = None;
                }
                self.state = Some(state);
            }
            Event::Color { state, after_write } => {
                self.color = Some(Observation {
                    color: state,
                    after_write,
                    sequence: self.sequence,
                })
            }
            Event::ColorUnavailable => self.color = None,
            Event::SendRejected { revision } => self.rejected = Some(revision),
            Event::Push(_) | Event::Sent { .. } => unreachable!(),
        }
        true
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Resolution {
    #[default]
    None,
    Pull,
    Send,
}
struct Pending {
    revision: u64,
    color: ColorCommand,
    started: Instant,
}
pub(crate) struct Sync {
    generation: u64,
    previous: ColorEdit,
    mapping: GamutMapping,
    dirty: Option<u64>,
    pending: Option<Pending>,
    protected: bool,
    revision: u64,
    applied: u64,
}
#[derive(Default)]
pub(crate) struct Feedback {
    pub hint: String,
    pub resolve: bool,
}
impl Sync {
    pub fn new(model: PickerModel) -> Self {
        Self {
            generation: 0,
            previous: model.edit.get_untracked(),
            mapping: model.mapping.get_untracked(),
            dirty: None,
            pending: None,
            protected: false,
            revision: 0,
            applied: 0,
        }
    }
    pub fn tick(
        &mut self,
        model: PickerModel,
        inbox: &Inbox,
        generation: u64,
        resolution: Resolution,
        now: Instant,
        mut send: impl FnMut(u64, u64, ColorCommand) -> bool,
    ) -> Feedback {
        let valid = inbox.generation == generation;
        let active = matches!(
            model.companion.state.get_untracked(),
            State::Scanning | State::Connecting | State::Connected | State::Reconnecting
        );
        if valid && self.generation != generation {
            model.finish_remote();
            if inbox.requested {
                self.protected = self.dirty == Some(generation) && self.protected;
                if self.dirty != Some(generation) {
                    self.dirty = None;
                }
            } else if self.pending.is_some() || self.dirty.is_some() {
                self.protected = true;
            }
            self.pending = None;
            self.generation = generation;
            self.applied = 0;
        }
        let edit = model.edit.get_untracked();
        let mapping = model.mapping.get_untracked();
        let color_changed = edit.origin == EditOrigin::Local
            && edit.color.components()[..3] != self.previous.color.components()[..3];
        if active && (color_changed || mapping != self.mapping) {
            self.dirty = Some(generation);
            self.pending = None;
            // Offline edits remain local until an explicit reconciliation action.
            self.protected = !valid || inbox.state != Some(State::Connected);
        }
        self.previous = edit;
        self.mapping = mapping;
        if !valid || inbox.state != Some(State::Connected) {
            model.finish_remote();
            return Feedback::default();
        }
        let Some(observation) = inbox.color else {
            model.finish_remote();
            return Feedback {
                hint: "Waiting for CSP color state".into(),
                resolve: false,
            };
        };
        let remote = observation.color;
        let busy = model.dragging.get_untracked() || model.editing_text.get_untracked();
        if busy || self.dirty.is_some() || self.protected || self.pending.is_some() {
            model.finish_remote();
        }
        if resolution == Resolution::Pull && !busy && !remote.manipulating {
            self.dirty = None;
            self.pending = None;
            self.protected = false;
            // Explicitly adopting CSP also resolves a same-RGB wide-gamut source.
            model.receive_rgb(remote.rgb);
            self.previous = model.edit.get_untracked();
            self.applied = observation.sequence;
        } else if resolution == Resolution::Send {
            self.dirty = Some(generation);
            self.pending = None;
            self.protected = false;
        }
        if let Some(pending) = &self.pending
            && inbox.rejected == Some(pending.revision)
        {
            self.protected = true;
        }
        if self.dirty.is_some_and(|dirty| dirty != generation) {
            self.protected = true;
        }
        if self.dirty == Some(generation)
            && !self.protected
            && remote.can_set
            && !remote.manipulating
        {
            let Ok(rgb) = model.current().mapped_rgb(Gamut::Srgb, mapping) else {
                return Feedback {
                    hint: "Cannot represent this color for CSP".into(),
                    resolve: true,
                };
            };
            let Ok(color) = ColorCommand::new(rgb, remote.slot, false) else {
                return Feedback {
                    hint: "Cannot represent this color for CSP".into(),
                    resolve: true,
                };
            };
            let Some(revision) = self.revision.checked_add(1) else {
                return Feedback {
                    hint: "CSP edit counter exhausted; restart the picker".into(),
                    resolve: false,
                };
            };
            if send(generation, revision, color) {
                self.revision = revision;
                self.dirty = None;
                self.pending = Some(Pending {
                    revision,
                    color,
                    started: now,
                });
            }
        }
        if let Some(pending) = &self.pending {
            if !self.protected
                && observation.after_write >= pending.revision
                && remote.slot == pending.color.slot()
                && remote.transparent == pending.color.transparent()
                && same_rgb(remote.rgb, pending.color.rgb())
            {
                self.pending = None;
                self.applied = observation.sequence;
                // Keep every original f64 component, including neutral hue and alpha.
            } else if self.protected
                || now.saturating_duration_since(pending.started) >= Duration::from_secs(2)
            {
                self.protected = true;
                return Feedback {
                    hint: "CSP color not confirmed; local color retained".into(),
                    resolve: !busy && !remote.manipulating,
                };
            } else {
                return Feedback {
                    hint: "Sending color to CSP…".into(),
                    resolve: false,
                };
            }
        }
        if self.protected || self.dirty.is_some() {
            return Feedback {
                hint: "Local color retained; choose how to sync".into(),
                resolve: !busy && !remote.manipulating,
            };
        }
        if !busy && observation.sequence != self.applied {
            let matches = model
                .current()
                .mapped_rgb(Gamut::Srgb, mapping)
                .is_ok_and(|rgb| same_rgb(rgb, remote.rgb));
            if !matches {
                model.receive_rgb_sample(remote.rgb, remote.manipulating);
            }
            if !remote.manipulating {
                model.finish_remote();
            }
            self.previous = model.edit.get_untracked();
            self.applied = observation.sequence;
        }
        Feedback {
            hint: format!(
                "CSP {}{} · Alpha stays local",
                if remote.slot == 0 { "main" } else { "sub" },
                if remote.transparent {
                    " / transparent"
                } else {
                    ""
                }
            ),
            resolve: false,
        }
    }
}
fn same_rgb(left: [f64; 3], right: [f64; 3]) -> bool {
    // CSP's displayed RGB may be quantized to 8-bit even on its uint32 wire.
    left.into_iter()
        .zip(right)
        .all(|(a, b)| a.is_finite() && b.is_finite() && (a * 255.0).round() == (b * 255.0).round())
}

#[cfg(test)]
mod tests {
    use super::*;
    use okcolor_core::Oklch;
    fn remote(rgb: [f64; 3]) -> ColorState {
        ColorState {
            rgb,
            slot: 0,
            transparent: false,
            can_set: true,
            manipulating: false,
        }
    }
    fn inbox(color: ColorState) -> Inbox {
        let mut inbox = Inbox::default();
        inbox.absorb(1, Event::State(State::Connecting));
        inbox.absorb(1, Event::State(State::Connected));
        inbox.absorb(
            1,
            Event::Color {
                state: color,
                after_write: 0,
            },
        );
        inbox
    }
    fn setup() -> (PickerModel, Sync, Inbox) {
        let model =
            PickerModel::new(Oklch::from_rgb([0.2; 3], Gamut::Srgb, 0.37, 123.456).unwrap());
        model.companion.state.set(State::Connected);
        let sync = Sync::new(model);
        let inbox = inbox(remote([0.2; 3]));
        (model, sync, inbox)
    }
    fn tick(
        sync: &mut Sync,
        model: PickerModel,
        inbox: &Inbox,
        now: Instant,
        resolution: Resolution,
        sent: &mut Vec<(u64, u64, ColorCommand)>,
    ) -> Feedback {
        sync.tick(
            model,
            inbox,
            inbox.generation,
            resolution,
            now,
            |g, r, c| {
                sent.push((g, r, c));
                true
            },
        )
    }
    #[test]
    fn receiving_a_color_preserves_alpha_neutral_hue_and_does_not_echo() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            let before = model.current();
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            inbox.absorb(
                1,
                Event::Color {
                    state: remote([1.0, 0.0, 0.0]),
                    after_write: 0,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current().components()[3], 0.37);
            assert_eq!(model.edit.get_untracked().origin, EditOrigin::Companion);
            assert_eq!(model.recent.get_untracked().len(), 1);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert!(sent.is_empty());
            model.undo();
            assert_eq!(model.current(), before);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(sent.len(), 1);
            assert_eq!(model.current().components()[2], 123.456);
        });
    }
    #[test]
    fn quantized_echo_and_old_observations_preserve_every_original_component() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            let original = Oklch::new(
                0.6543210987654321,
                0.3210987654321098,
                123.4567890123456,
                0.3456789012345678,
            )
            .unwrap();
            model.select(original);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            let snapshot = model.saved_state_untracked();
            let (_, revision, command) = sent[0];
            inbox.absorb(
                1,
                Event::Color {
                    state: remote([0.2; 3]),
                    after_write: 0,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current(), original);
            let echo = command.rgb().map(|v| (v * 255.0).round() / 255.0);
            inbox.absorb(
                1,
                Event::Color {
                    state: remote(echo),
                    after_write: revision,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.saved_state_untracked(), snapshot);
            assert_eq!(sent.len(), 1);
            // A later repeated poll must not replace the precise source either.
            inbox.absorb(
                1,
                Event::Color {
                    state: remote(echo),
                    after_write: revision,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current(), original);
        });
    }
    #[test]
    fn canceling_a_drag_sends_the_original_without_accepting_an_older_echo() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            let before = model.current();
            model.begin();
            model.preview(Oklch::new(0.7, 0.2, 27.0, 0.12).unwrap());
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            let old = sent[0];
            model.cancel();
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(sent.len(), 2);
            inbox.absorb(
                1,
                Event::Color {
                    state: remote(old.2.rgb()),
                    after_write: old.1,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current(), before);
            assert!(!model.can_undo.get_untracked());
        });
    }
    #[test]
    fn text_focus_defers_remote_color_and_a_local_commit_wins_the_race() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            let before = model.current();
            model.editing_text.set(true);
            inbox.absorb(
                1,
                Event::Color {
                    state: remote([0.0, 1.0, 0.0]),
                    after_write: 0,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current(), before);
            model.input("#00f");
            let local = model.current();
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(sent.len(), 1);
            model.editing_text.set(false);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current(), local);
        });
    }
    #[test]
    fn reconnect_never_replays_or_overwrites_an_unconfirmed_local_edit() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            model.input("oklch(.7 .2 40 / .23)");
            let local = model.current();
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            inbox.absorb(2, Event::State(State::Reconnecting));
            model.companion.state.set(State::Reconnecting);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            inbox.absorb(2, Event::State(State::Connected));
            model.companion.state.set(State::Connected);
            inbox.absorb(
                2,
                Event::Color {
                    state: remote([0.0, 1.0, 0.0]),
                    after_write: 0,
                },
            );
            assert!(tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent).resolve);
            assert_eq!(model.current(), local);
            assert_eq!(sent.len(), 1);
            tick(&mut sync, model, &inbox, now, Resolution::Send, &mut sent);
            assert_eq!(sent.len(), 2);
            inbox.absorb(
                2,
                Event::SendRejected {
                    revision: sent[1].1,
                },
            );
            assert!(tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent).resolve);
            tick(&mut sync, model, &inbox, now, Resolution::Pull, &mut sent);
            assert_eq!(model.current().components()[3], 0.23);
            assert_eq!(sent.len(), 2);
            assert_eq!(model.edit.get_untracked().origin, EditOrigin::Companion);
            model.undo();
            assert_eq!(model.current(), local);
        });
    }
    #[test]
    fn timeout_retains_source_and_new_local_edit_can_resolve_it() {
        Owner::new().with(|| {
            let (model, mut sync, inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            model.input("#f00");
            let local = model.current();
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert!(
                tick(
                    &mut sync,
                    model,
                    &inbox,
                    now + Duration::from_secs(3),
                    Resolution::None,
                    &mut sent
                )
                .resolve
            );
            assert_eq!(model.current(), local);
            model.input("#00f");
            tick(
                &mut sync,
                model,
                &inbox,
                now + Duration::from_secs(3),
                Resolution::None,
                &mut sent,
            );
            assert_eq!(sent.len(), 2);
        });
    }
    #[test]
    fn opacity_is_local_and_remote_drag_becomes_one_undo_commit() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            model.set_channel(3, 0.0);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert!(sent.is_empty());
            let before = model.current();
            let count = model.recent.get_untracked().len();
            let mut color = remote([1.0, 0.0, 0.0]);
            color.manipulating = true;
            color.transparent = true;
            inbox.absorb(
                1,
                Event::Color {
                    state: color,
                    after_write: 0,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_ne!(model.current(), before);
            assert_eq!(model.current().components()[3], 0.0);
            assert!(
                !model.dragging.get_untracked(),
                "remote preview must not block subsequent samples"
            );
            assert_eq!(model.recent.get_untracked().len(), count);
            assert_eq!(model.saved_state_untracked().current, before);
            for step in 1..20 {
                color.rgb = [1.0 - f64::from(step) / 20.0, f64::from(step) / 20.0, 0.0];
                inbox.absorb(
                    1,
                    Event::Color {
                        state: color,
                        after_write: 0,
                    },
                );
                let previous = model.current();
                tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
                assert_ne!(model.current(), previous);
                assert_eq!(model.edit.get_untracked().origin, EditOrigin::Companion);
                assert_eq!(model.recent.get_untracked().len(), count);
                assert!(sent.is_empty());
            }
            color.rgb = [0.0, 1.0, 0.0];
            color.manipulating = false;
            inbox.absorb(
                1,
                Event::Color {
                    state: color,
                    after_write: 0,
                },
            );
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current().components()[3], 0.0);
            assert_eq!(model.recent.get_untracked().len(), count + 1);
            assert!(sent.is_empty());
            model.undo();
            assert_eq!(model.current(), before);
        });
    }
    #[test]
    fn edits_made_during_reconnection_require_explicit_reconciliation() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            inbox.absorb(2, Event::State(State::Reconnecting));
            model.companion.state.set(State::Reconnecting);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            model.input("#f00");
            let edited = model.current();
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            inbox.absorb(2, Event::State(State::Connected));
            model.companion.state.set(State::Connected);
            inbox.absorb(
                2,
                Event::Color {
                    state: remote([0.0, 0.0, 1.0]),
                    after_write: 0,
                },
            );
            assert!(tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent).resolve);
            assert!(sent.is_empty());
            assert_eq!(model.current(), edited);
        });
    }

    #[test]
    fn a_remote_preview_commits_before_local_takeover_or_disconnect() {
        for disconnect in [false, true] {
            Owner::new().with(|| {
                let (model, mut sync, mut inbox) = setup();
                let now = Instant::now();
                let mut sent = vec![];
                tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
                let before = model.current();
                let mut color = remote([1.0, 0.0, 0.0]);
                color.manipulating = true;
                inbox.absorb(
                    1,
                    Event::Color {
                        state: color,
                        after_write: 0,
                    },
                );
                tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
                let preview = model.current();
                assert_ne!(preview, before);
                if disconnect {
                    inbox.absorb(2, Event::State(State::Reconnecting));
                    model.companion.state.set(State::Reconnecting);
                    tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
                } else {
                    assert!(model.begin());
                    model.preview(Oklch::new(0.5, 0.08, 30.0, 0.4).unwrap());
                    tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
                    model.commit();
                    model.undo();
                }
                assert_eq!(model.current(), preview);
                assert_eq!(model.saved_state_untracked().current, preview);
                model.undo();
                assert_eq!(model.current(), before);
                assert!(sent.is_empty(), "remote gestures must not echo");
            });
        }
    }
    #[test]
    fn an_unmodified_text_draft_defers_the_latest_neutral_color_until_blur() {
        Owner::new().with(|| {
            let (model, mut sync, mut inbox) = setup();
            let now = Instant::now();
            let mut sent = vec![];
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            let before = model.current();
            model.editing_text.set(true);
            for value in [0.4, 0.6, 0.8] {
                inbox.absorb(
                    1,
                    Event::Color {
                        state: remote([value; 3]),
                        after_write: 0,
                    },
                );
                tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
                assert_eq!(model.current(), before);
            }
            model.editing_text.set(false);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current().components()[2], before.components()[2]);
            assert_eq!(model.current().components()[3], before.components()[3]);
            assert_eq!(model.recent.get_untracked().len(), 1);
            assert!(sent.is_empty());
            assert!(same_rgb(model.current().encoded_rgb(Gamut::Srgb), [0.8; 3]));
        });
    }
    #[test]
    fn output_mapping_change_sends_a_projection_without_rewriting_the_source() {
        Owner::new().with(|| {
            let original = Oklch::new(0.7, 0.4, 35.0, 0.41).unwrap();
            let model = PickerModel::new(original);
            model.companion.state.set(State::Connected);
            let mut sync = Sync::new(model);
            let now = Instant::now();
            let mut sent = vec![];
            let inbox = inbox(remote(
                original
                    .mapped_rgb(Gamut::Srgb, model.mapping.get_untracked())
                    .unwrap(),
            ));
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(model.current(), original);
            model.mapping.set(GamutMapping::Clip);
            tick(&mut sync, model, &inbox, now, Resolution::None, &mut sent);
            assert_eq!(sent.len(), 1);
            assert!(same_rgb(
                sent[0].2.rgb(),
                original
                    .mapped_rgb(Gamut::Srgb, GamutMapping::Clip)
                    .unwrap()
            ));
            assert_eq!(model.current(), original);
            assert!(!model.can_undo.get_untracked());
        });
    }
}
