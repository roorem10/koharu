//! Booharu's code in koharu-runtime. Its one hook, in `store::root()`, gives
//! the store Booharu's own fallback folder: where Koharu's root, the folder
//! the program is in, can't be written or isn't used (any build but a Windows
//! release build), the store is the OS cache folder's `booharu/store`, never
//! Koharu's `koharu/store`. So an installed Koharu and Booharu never share a
//! store, where each start would remove the runtime versions the other uses
//! (`store::remove_others`). Booharu's per-user install and its portable
//! folder can be written, so their store stays beside the program; on Windows
//! the cache folder is the local app data folder, so the fallback store sits
//! beside the projects' own fallback, the local app data folder's `Booharu`.

#[cfg(test)]
mod tests {
    #[test]
    fn the_store_falls_back_to_booharus_own_folder_in_the_cache() {
        let store = crate::store::root();
        match crate::root() {
            Some(root) => assert_eq!(store, root.join("store")),
            None => {
                let cache = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
                assert_eq!(store, cache.join("booharu").join("store"));
            }
        }
    }
}
