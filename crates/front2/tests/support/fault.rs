#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    CorruptPull,
    CorruptPush,
    SecondWriteFails,
    RollbackFails,
    ReportsVerifiedWithStaleVbmeta,
    NeverFinishes,
}
