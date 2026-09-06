pub mod catalog;
pub mod engine;
pub mod generation;
pub mod install;
pub mod manager;
pub mod media;
pub mod provider;
pub mod store;
pub mod tensor;
pub mod transport;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/_.rs"));
}

#[allow(warnings, clippy::all)]
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/config_generated.rs"));
}
