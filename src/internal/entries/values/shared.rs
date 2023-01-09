use crate::internal::get_current_timestamp;
use std::io;

pub(crate) trait ValueEntry<'a>: Sized {
    /// Gets the expiry of the value entry
    fn get_expiry(&self) -> u64;

    /// Extracts the value entry from the data array
    fn from_data_array(data: &'a [u8], offset: usize) -> io::Result<Self>;

    /// Retrieves the byte array that represents the value entry.
    fn as_bytes(&self) -> Vec<u8>;

    /// Returns true if key has lived for longer than its time-to-live
    /// It will always return false if time-to-live was never set
    fn is_expired(&self) -> bool {
        let expiry = self.get_expiry();
        if expiry == 0 {
            false
        } else {
            expiry < get_current_timestamp()
        }
    }
}
