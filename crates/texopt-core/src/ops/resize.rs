use serde::{Deserialize, Serialize};

use super::OpOutput;
use crate::{ImageBuf, OpError, OpResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ResizeParams {}

pub fn apply(_img: &ImageBuf, _params: &ResizeParams) -> OpResult<OpOutput> {
    Err(OpError::not_implemented("resize"))
}
