/// Creates the database folder if it does not exist
pub(crate) fn initialize_file_db(store_path: &str) {
    let _ = std::fs::create_dir_all(store_path);
}