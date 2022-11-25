mod msg;
mod query;

pub use msg::FuryMessages;
pub use query::{
    FuryQuery, GetAppResponse, GetAssetDataResponse, MessageValidateResponse, StateResponse,
    TotalSupplyResponse,
};

// This is a signal, such that any contract that imports these helpers will only run on the
// fury blockchain
#[no_mangle]
extern "C" fn requires_fury() {}
