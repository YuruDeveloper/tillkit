#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutput {
    pub stdout: String,
    pub status: i32,
}

impl ShellOutput {
    pub fn new(stdout: impl Into<String>, status: i32) -> Self {
        Self {
            stdout: stdout.into(),
            status,
        }
    }

    pub fn succeeded(&self) -> bool {
        self.status == 0
    }

    pub fn text(&self) -> &str {
        self.stdout.trim()
    }
}
