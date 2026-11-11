use front2::{DeviceState, SupportReport};

#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub state: DeviceState,
    pub report: SupportReport,
}
