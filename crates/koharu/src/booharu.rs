//! Booharu's glue in the program's own crate: Sentry stays off in a build that compiled in no
//! DSN, as Booharu's are.

use sentry::ClientOptions;

/// Takes `SENTRY_DSN` out of the program's environment when `options`, about to start Sentry,
/// have no DSN, as in a build that compiled none in: `scripts/build-preview-windows.ps1` removes
/// `SENTRY_*` before Cargo runs, and `PREVIEW_INSTALL.md` says Sentry reporting is off. Sentry's
/// defaults would otherwise take a DSN set on the machine the program runs on, and send it every
/// error, trace, session and crash dump, each error with the window's picture. A DSN compiled
/// in, as Koharu's releases have, leaves the environment alone.
pub(crate) fn keep_off(options: &ClientOptions) {
    if options.dsn.is_none() {
        // SAFETY: `sentry::initialize`, the only caller, runs first in `main`, before the async
        // runtime or any other thread starts, so nothing reads the environment meanwhile.
        unsafe { std::env::remove_var("SENTRY_DSN") };
    }
}

#[cfg(test)]
mod tests {
    use sentry::{ClientOptions, Hub, IntoDsn as _};

    use super::keep_off;

    /// A loopback port nothing listens on, so a Sentry started with it sends nothing anywhere.
    const DSN: &str = "http://public@127.0.0.1:9/1";

    /// Whether Sentry's defaults would take a DSN from the environment.
    fn taken() -> bool {
        sentry::apply_defaults(ClientOptions::new()).dsn.is_some()
    }

    #[test]
    fn a_build_without_a_dsn_stays_off_with_one_set_where_it_runs() {
        // SAFETY: the crate's only test, so no other thread reads the environment meanwhile.
        unsafe { std::env::set_var("SENTRY_DSN", DSN) };
        assert!(taken());

        // A DSN compiled in leaves the environment's alone.
        let mut compiled = ClientOptions::new();
        compiled.dsn = DSN.into_dsn().expect("the DSN parses");
        keep_off(&compiled);
        assert!(taken());

        if option_env!("SENTRY_DSN").is_some() {
            // Built with a DSN, as Koharu's releases are: starting Sentry would report to it.
            keep_off(&ClientOptions::new());
            assert!(!taken());
            return;
        }
        // Built without one, as Booharu always is: Sentry starts with the environment's DSN
        // gone and sends nothing, and its client stays bound, as the Sentry plugin's setup needs.
        // This test is the first to touch Sentry's hubs, so its thread's is the main one.
        let guard = crate::sentry::initialize();
        assert!(!guard.is_enabled());
        assert!(!taken());
        assert!(
            Hub::main()
                .client()
                .is_some_and(|client| !client.is_enabled())
        );
    }
}
