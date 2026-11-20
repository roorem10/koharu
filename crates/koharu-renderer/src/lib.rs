//! Renders Koharu scene pages with vello: layout, retained layer content,
//! and the GPU passes that draw it.

// Booharu: each font family's names in other languages, for its font picker,
// the face a bold or italic text takes, and lines of mixed sizes.
pub mod booharu;
mod config;
pub mod fonts;
mod frame;
mod gpu;
mod pixels;
mod renderer;
mod script;
mod text;

pub use config::RendererConfig;
pub use frame::{Drawn, Frame, Raster};
pub use gpu::{Drawing, Gpu};
pub use renderer::Renderer;
pub use text::{Label, Lettering, ResolvedText, TextRun};
