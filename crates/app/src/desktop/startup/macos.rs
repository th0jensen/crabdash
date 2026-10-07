use anyhow::{Result, bail};
pub(super) const SUPPORTED: bool = false;
pub(super) fn startup_enabled() -> Result<bool> {
    Ok(false)
}
pub(super) fn set_login_startup(_: bool) -> Result<()> {
    bail!("Login startup is supported on Linux")
}
