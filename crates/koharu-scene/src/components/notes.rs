use revision::revisioned;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    Error, Result,
    component::{Component, ValidationContext},
};

use super::Origin;

/// An annotation on a text box, independent of semantic text and typography.
#[revisioned(revision = 1)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct BoxNote {
    pub origin: Origin,
    pub text: String,
}

impl Component for BoxNote {
    const KIND: &'static str = "dev.booharu.box.note";

    fn validate(&self, _context: &ValidationContext<'_>) -> Result<()> {
        self.origin.validate()?;
        if self.text.trim().is_empty()
            || self.text.len() > 16 * 1024 * 1024
            || self.text.contains('\0')
        {
            return Err(Error::invalid("box note is empty, too large, or contains NUL"));
        }
        Ok(())
    }

    fn origin(&self) -> Option<&Origin> {
        Some(&self.origin)
    }

    fn set_origin(&mut self, origin: Origin) -> bool {
        self.origin = origin;
        true
    }
}
