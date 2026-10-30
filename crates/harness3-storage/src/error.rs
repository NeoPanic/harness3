pub type PResult<T> = Result<T, Fault>;
#[derive(Debug)]
pub struct Fault(pub u16, pub String);
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}
impl std::error::Error for Fault {}
impl From<rusqlite::Error> for Fault {
    fn from(_: rusqlite::Error) -> Self {
        Self(500, "storage_error".into())
    }
}
impl From<serde_json::Error> for Fault {
    fn from(_: serde_json::Error) -> Self {
        Self(500, "stored_data_invalid".into())
    }
}
impl From<harness3_domain::Fault> for Fault {
    fn from(error: harness3_domain::Fault) -> Self {
        Self(error.0, error.1)
    }
}
impl From<Fault> for harness3_domain::Fault {
    fn from(error: Fault) -> Self {
        Self(error.0, error.1)
    }
}
pub fn fault(code: u16, message: &str) -> Fault {
    Fault(code, message.into())
}
