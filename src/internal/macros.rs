pub(crate) use acquire_lock;

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
