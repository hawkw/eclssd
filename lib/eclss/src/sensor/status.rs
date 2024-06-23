use core::{
    fmt,
    sync::atomic::{AtomicU8, Ordering},
};
#[cfg(feature = "serde")]
use serde::{Serialize, Serializer};

pub use eclss_api::SensorStatus as Status;

pub struct StatusCell(AtomicU8);

impl StatusCell {
    pub const fn new() -> Self {
        Self(AtomicU8::new(Status::Unknown as u8))
    }

    pub fn set_status(&self, status: Status) -> Status {
        let prev = self.0.swap(status as u8, Ordering::AcqRel);
        Status::from_u8(prev)
    }

    #[must_use]
    pub fn status(&self) -> Status {
        Status::from_u8(self.0.load(Ordering::Acquire))
    }
}

impl fmt::Debug for StatusCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("StatusCell").field(&self.status()).finish()
    }
}

#[cfg(feature = "serde")]
impl Serialize for StatusCell {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.status().serialize(serializer)
    }
}

impl Default for StatusCell {
    fn default() -> Self {
        Self::new()
    }
}
