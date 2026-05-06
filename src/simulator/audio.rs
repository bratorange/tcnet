use rodio::{OutputStream, OutputStreamHandle, Sink};

pub struct AudioEngine {
    _stream: OutputStream,
    pub handle: OutputStreamHandle,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (_stream, handle) = OutputStream::try_default()
            .expect("Could not open audio output device");
        Self { _stream, handle }
    }

    pub fn new_sink(&self) -> Sink {
        Sink::try_new(&self.handle).expect("Could not create audio sink")
    }
}
