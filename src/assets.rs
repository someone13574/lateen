use std::borrow::Cow;

use gpui::{App, AssetSource, Result, SharedString};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "assets"]
#[include = "fonts/*.ttf"]
#[include = "icons/*.svg"]
pub struct Assets;

impl Assets {
    pub fn load_fonts(cx: &App) -> Result<()> {
        let fonts = Self::iter()
            .filter(|path| path.ends_with(".ttf"))
            .filter_map(|path| cx.asset_source().load(&path).ok().flatten())
            .collect();

        cx.text_system().add_fonts(fonts)
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::get(path).map(|file| file.data))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|entry| entry.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}
