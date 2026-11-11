#[derive(Debug, Clone, Copy)]
pub struct PairRequest<'a> {
    pub id: &'a str,
    pub created_unix: u64,
    pub note: &'a str,
}
