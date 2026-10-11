use super::{Command, Snapshot, Transport};
use crate::synthesis::audio::{PianoControl, Player};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

pub(super) fn run(
    rx: Receiver<Command>,
    published: Arc<Mutex<Snapshot>>,
    pending: Arc<AtomicU64>,
    piano: PianoControl,
    repaint: eframe::egui::Context,
) {
    let mut player = None;
    let mut command_error = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Command::Audition) => {
                let result = ensure_player(&mut player, &piano)
                    .and_then(|player| player.start_output().map_err(|error| error.to_string()));
                if let Ok(mut status) = published.lock() {
                    status.audition_error =
                        result.err().map(|error| format!("钢琴试听失败：{error}"));
                }
                repaint.request_repaint();
            }
            Ok(Command::Transport(serial, command)) => {
                let resets_error = !matches!(&command, Transport::Control { .. });
                let result = apply(command, &mut player, serial, &pending, &piano);
                match result {
                    Err(error) => command_error = Some(error),
                    Ok(())
                        if resets_error
                            || player.as_ref().and_then(Player::position_seconds).is_some() =>
                    {
                        command_error = None
                    }
                    Ok(()) => {}
                }
                if let Ok(mut status) = published.lock()
                    && serial == pending.load(Ordering::Acquire)
                {
                    status.serial = serial;
                    status.error = command_error.clone();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if let Ok(mut status) = published.lock()
            && status.serial == pending.load(Ordering::Acquire)
        {
            status.control = player.as_ref().and_then(Player::control);
            status.ready = player.as_ref().and_then(Player::position_seconds).is_some();
            status.playing = player.as_ref().is_some_and(Player::is_playing);
            status.buffering = player.as_ref().is_some_and(Player::buffering);
            status.position_seconds = player.as_ref().and_then(Player::position_seconds);
            if let Some(error) = player.as_ref().and_then(Player::error) {
                let error = error.to_string();
                if status.error.as_ref() != Some(&error) {
                    status.error = Some(error);
                    repaint.request_repaint();
                }
            }
        }
    }
    piano.set_note(None);
}

fn ensure_player<'a>(
    player: &'a mut Option<Player>,
    piano: &PianoControl,
) -> Result<&'a mut Player, String> {
    if player.is_none() {
        *player =
            Some(Player::new_default_with_piano(piano.clone()).map_err(|error| error.to_string())?);
    }
    Ok(player.as_mut().expect("created player"))
}

fn apply(
    command: Transport,
    player: &mut Option<Player>,
    serial: u64,
    latest: &AtomicU64,
    piano: &PianoControl,
) -> Result<(), String> {
    match command {
        Transport::Load(audio, seconds, playing) => {
            let player = ensure_player(player, piano)?;
            player.pause();
            if let Err(error) = player.load_project(audio) {
                player.clear();
                return Err(error.to_string());
            }
            // A queued pause/play without a seek must retain the Load target,
            // even when that newer command superseded this command's serial.
            player.seek(seconds).map_err(|error| error.to_string())?;
            if serial != latest.load(Ordering::Acquire) {
                return Ok(());
            }
            if playing {
                player.play().map_err(|error| error.to_string())?;
            }
        }
        Transport::Control { playing, seek } => {
            if let Some(player) = player.as_ref()
                && player.position_seconds().is_some()
            {
                if !playing {
                    player.pause();
                }
                if let Some(seconds) = seek {
                    player.seek(seconds).map_err(|error| error.to_string())?;
                }
                // Later pause/play commands inherit an earlier queued seek.
                // Only the latest command may start output, but every seek
                // applies in order while source installation is pending.
                if serial != latest.load(Ordering::Acquire) {
                    return Ok(());
                }
                if playing {
                    player.play().map_err(|error| error.to_string())?;
                } else {
                    player.pause();
                }
            }
        }
        Transport::Clear => {
            if let Some(player) = player.as_mut() {
                player.clear();
            }
        }
    }
    Ok(())
}
