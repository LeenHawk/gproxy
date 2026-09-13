//! Token-count request/response mappings. Counts always describe the selected
//! target model's tokenizer; conversion cannot estimate a different tokenizer.

mod response;
pub use response::*;
