//! Native browser protocol emulation for authenticated, read-only Desktop probes.
use std::time::Duration;

/// No cookie jar: callers supply a transient header and no response changes the
/// source profile. Redirects are disabled so authentication stays on its host.
pub fn claude_desktop_client() -> wreq::Result<wreq::Client> {
    wreq::Client::builder()
        .emulation(wreq_util::Emulation::Chrome137)
        .redirect(wreq::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .build()
}
