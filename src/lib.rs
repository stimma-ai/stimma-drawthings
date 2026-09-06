pub mod engine;
pub mod tensor;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/_.rs"));
}

#[allow(warnings, clippy::all)]
pub mod generated {
    include!("generated/config_generated.rs");
}
