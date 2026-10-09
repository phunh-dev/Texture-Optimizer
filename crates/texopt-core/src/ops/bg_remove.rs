use serde::{Deserialize, Serialize};

use super::OpOutput;
use crate::{ImageBuf, OpError, OpResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BgRemoveParams {}

pub fn apply(_img: &ImageBuf, _params: &BgRemoveParams) -> OpResult<OpOutput> {
    Err(OpError::not_implemented("bg_remove"))
}
