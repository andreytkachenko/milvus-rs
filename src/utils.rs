use crate::{
    error::Error,
    proto::common::{ErrorCode, MsgBase, MsgType, Status},
};

pub fn new_msg(mtype: MsgType) -> MsgBase {
    MsgBase {
        msg_type: mtype as i32,
        timestamp: 0,
        source_id: 0,
        msg_id: 0,
    }
}

pub fn status_to_result(status: Option<Status>) -> Result<(), Error> {
    let status = status.ok_or(Error::Unknown)?;

    match ErrorCode::from_i32(status.error_code) {
        Some(i) => match i {
            ErrorCode::Success => Ok(()),
            _ => Err(Error::from(status)),
        },
        None => Err(Error::Unknown),
    }
}
