//! Keep atlas uploads pending while the device is unavailable.
use zgui::{
    atlas::{SinkError, TextureFormat, TextureId, TextureSink},
    geom::{Device, Rect, Size},
};

pub(crate) struct UnavailableSink;

impl TextureSink for UnavailableSink {
    fn begin_uploads(&mut self) -> Result<(), SinkError> {
        Err(SinkError::new("Graphics device is being recovered"))
    }
    fn create_texture(
        &mut self,
        _: TextureId,
        _: Size<i32, Device>,
        _: TextureFormat,
    ) -> Result<(), SinkError> {
        self.begin_uploads()
    }
    fn write_texture(
        &mut self,
        _: TextureId,
        _: Rect<i32, Device>,
        _: TextureFormat,
        _: &[u8],
    ) -> Result<(), SinkError> {
        self.begin_uploads()
    }
    fn destroy_texture(&mut self, _: TextureId) {}
}
