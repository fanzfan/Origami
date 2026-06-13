//! 取 macOS 系统对某文件类型/目录的真实图标，编码为 PNG 字节。
//!
//! 必须在主线程调用（AppKit 约束），由 lib.rs 的命令通过 run_on_main_thread 调度。

use objc2::AnyThread;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSImageName, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSString};

/// 返回某扩展名（不含点）对应的系统图标 PNG。is_dir=true 时返回文件夹图标。
#[allow(deprecated)]
pub fn icon_png(ext: &str, is_dir: bool) -> Option<Vec<u8>> {
    unsafe {
        let image: objc2::rc::Retained<NSImage> = if is_dir {
            // NSImageNameFolder：系统通用文件夹图标
            let name: &NSImageName = objc2_app_kit::NSImageNameFolder;
            NSImage::imageNamed(name)?
        } else {
            let ws = NSWorkspace::sharedWorkspace();
            ws.iconForFileType(&NSString::from_str(ext))
        };

        let tiff = image.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)?;
        let props = NSDictionary::new();
        let png = rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props)?;

        // 经由临时文件取出字节，避免不稳定的裸指针读取。
        let tmp = std::env::temp_dir().join(format!(
            "origami-icon-{}.png",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let ok = png.writeToFile_atomically(&NSString::from_str(tmp.to_str()?), true);
        if !ok {
            return None;
        }
        let bytes = std::fs::read(&tmp).ok();
        let _ = std::fs::remove_file(&tmp);
        bytes
    }
}
