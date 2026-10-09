use serde::{Deserialize, Serialize};

use super::OpOutput;
use crate::{ImageBuf, OpError, OpResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PotPadParams {}

pub fn apply(_img: &ImageBuf, _params: &PotPadParams) -> OpResult<OpOutput> {
    Err(OpError::not_implemented("pot_pad"))
}
