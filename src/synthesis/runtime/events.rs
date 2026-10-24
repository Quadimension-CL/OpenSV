use super::worker::Event;
use eframe::egui;
use std::sync::mpsc::{SendError, Sender};

pub(super) struct Events {
    sender: Sender<Event>,
    repaint: egui::Context,
}

impl Events {
    pub fn new(sender: Sender<Event>, repaint: egui::Context) -> Self {
        Self { sender, repaint }
    }

    pub fn send(&self, event: Event) -> Result<(), SendError<Event>> {
        self.sender.send(event)?;
        self.repaint.request_repaint();
        Ok(())
    }
}

pub(super) struct WakeOnExit(pub egui::Context);

impl Drop for WakeOnExit {
    fn drop(&mut self) {
        self.0.request_repaint();
    }
}
