use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityLifecycleState {
    Enrolled { incarnation_id: Uuid },
    Withdrawn,
}

impl IdentityLifecycleState {
    pub const fn incarnation_id(self) -> Option<Uuid> {
        match self {
            Self::Enrolled { incarnation_id } => Some(incarnation_id),
            Self::Withdrawn => None,
        }
    }
}

pub fn display_name_or_handle<'a>(display_name: &'a str, handle: &'a str) -> &'a str {
    if display_name.trim().is_empty() {
        handle
    } else {
        display_name
    }
}
