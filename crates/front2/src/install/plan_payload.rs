#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPayload {
    pub file: String,
    pub data: Vec<u8>,
}
