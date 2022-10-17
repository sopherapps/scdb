#[rustfmt::skip::macros(acquire_lock)]
pub(crate) use acquire_lock;

/// Acquires the lock on a Mutex and returns an io Error if it fails
macro_rules! acquire_lock {
    ($v:expr) => {
        $v.lock().map_err(|e| {
            std::io::Error::new(
                io::ErrorKind::Other,
                format!("failed to acquire lock on database: {}", e),
            )
        })
    };
}
