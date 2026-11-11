use crate::support_override::SupportOverride;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRequest<'a> {
    pub id: &'a str,
    pub created_unix: u64,
    pub support_override: SupportOverride,
}

impl<'a> PlanRequest<'a> {
    pub fn strict(id: &'a str, created_unix: u64) -> Self {
        Self {
            id,
            created_unix,
            support_override: SupportOverride::none(),
        }
    }
}
