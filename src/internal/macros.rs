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

/// Checks if the given range is within bounds or else returns an InvalidData error
macro_rules! validate_bounds {
    (($actual_lower:expr, $actual_upper:expr), ($expected_lower:expr, $expected_upper:expr) $(,$message:expr)?) => {
        if $actual_lower < $expected_lower || $actual_upper > $expected_upper {
            let _msg = "";
            $(let _msg = $message;)?

            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} Span {}-{} is out of bounds for {}-{}",
                    _msg,
                    $actual_lower,
                    $actual_upper,
                    $expected_lower,
                    $expected_upper,
                ),
            ))
        } else {
            Ok(())
        }
    };
}

pub(crate) use acquire_lock;
pub(crate) use safe_slice;
pub(crate) use validate_bounds;
