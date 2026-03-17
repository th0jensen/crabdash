use anyhow::Result;

use crate::ServiceItem;

pub trait Disks {
    /// Lists all disks connected to the machine
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceItem>)`: The disks connected to the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_disks(&mut self) -> Result<Vec<ServiceItem>>;
}
