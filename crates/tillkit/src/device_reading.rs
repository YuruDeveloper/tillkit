use crate::diagnosis::Diagnosis;

#[derive(Debug, Clone)]
pub enum DeviceReading {
    Android(Diagnosis),
    Edl,
}
