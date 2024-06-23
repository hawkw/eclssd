#[cfg(feature = "tracing")]
macro_rules! trace {
    ($($arg:tt)*) => {
        tracing::trace!($($arg)*);
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! debug {
    ($($arg:tt)*) => {
        tracing::debug!($($arg)*)
    };
}
#[cfg(not(feature = "tracing"))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! info {
    ($($arg:tt)*) => {
        tracing::info!($($arg)*)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! info {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! warn {
    ($($arg:tt)*) => {
        tracing::warn!($($arg)*)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! warn {
    ($($arg:tt)*) => {};
}

macro_rules! error {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::error!($($arg)*)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! error {
    ($($arg:tt)*) => {};
}
