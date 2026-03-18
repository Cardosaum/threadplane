use super::super::*;
use strum::IntoStaticStr;

#[derive(Debug, Clone, Copy, ValueEnum, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum TaskStatusValue {
    Claimed,
    Completed,
    Open,
}

impl TaskStatusValue {
    pub(crate) fn as_str(self) -> &'static str {
        self.into()
    }
}
