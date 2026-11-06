//! Stand-in adapter for platforms without an output yet.

use super::{Devices, Error, Event, OutputOptions, Render};

const UNSUPPORTED: &str = "audio output is only implemented for Windows";

pub struct Stop;

impl Stop {
    pub fn new() -> Result<Self, Error> {
        Ok(Stop)
    }

    pub fn signal(&self) {}
}

pub fn devices() -> Result<Devices, Error> {
    Err(Error::new(UNSUPPORTED))
}

pub fn run(
    _options: &OutputOptions,
    _render: &mut dyn Render,
    _report: &mut dyn FnMut(Event),
    _stop: &Stop,
) -> Result<(), Error> {
    Err(Error::new(UNSUPPORTED))
}
