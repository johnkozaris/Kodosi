#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionInput(Vec<u8>);

impl SessionInput {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
