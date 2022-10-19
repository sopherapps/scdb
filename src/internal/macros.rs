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

/// Slices a slice safely, throwing an error if it goes out of bounds
macro_rules! safe_slice {
    ($data:expr, $start:expr, $end:expr, $max_len:expr) => {
        if $start >= $max_len || $end > $max_len {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("slice {} - {} out of bounds for {:?}", $start, $end, $data),
            ))
        } else {
            Ok(&$data[$start..$end])
        }
    };
}

pub(crate) use acquire_lock;
pub(crate) use safe_slice;
