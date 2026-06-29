pub mod capture;
pub mod playback;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;

// 10ms frame: 480 samples/channel * 2 channels = 960 interleaved f32
pub const FRAME_SIZE: usize = 960;

// Ring buffer capacity: 400ms
pub const RING_CAPACITY: usize = SAMPLE_RATE as usize * CHANNELS as usize * 400 / 1000;
