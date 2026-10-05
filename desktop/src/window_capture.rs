//! A user-requested snapshot of this app's native window, including Metal content.
//! Never enumerate windows or fall back to a desktop screenshot.

#[cfg(target_os = "macos")]
pub fn capture(window: &gpui_kit::Window) -> Result<Vec<u8>, String> {
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = HasWindowHandle::window_handle(window).map_err(|_| "Window unavailable.")?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Window unavailable.".into());
    };
    // GPUI owns this NSView. This function is called on the UI thread while its
    // window is being updated, so neither native object can disappear here.
    let number = unsafe {
        (&*handle.ns_view.as_ptr().cast::<NSView>())
            .window()
            .ok_or("Window unavailable.")?
            .windowNumber()
    };
    let number = u32::try_from(number)
        .ok()
        .filter(|number| *number != 0)
        .ok_or("Window unavailable.")?;
    native::capture(number)
}

#[cfg(not(target_os = "macos"))]
pub fn capture(_window: &gpui_kit::Window) -> Result<Vec<u8>, String> {
    Err("Window screenshot unavailable on this platform.".into())
}

#[cfg(target_os = "macos")]
mod native {
    use objc2_foundation::NSRect;
    use std::{ffi::c_void, ptr, slice};

    type Object = *const c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        static CGRectNull: NSRect;
        fn CGWindowListCreateImage(
            bounds: NSRect,
            options: u32,
            window: u32,
            image_options: u32,
        ) -> Object;
        fn CGImageGetWidth(image: Object) -> usize;
        fn CGImageGetHeight(image: Object) -> usize;
        fn CGImageGetDataProvider(image: Object) -> Object;
        fn CGDataProviderCopyData(provider: Object) -> Object;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: Object);
        fn CFDataCreateMutable(allocator: Object, capacity: isize) -> Object;
        fn CFDataGetLength(data: Object) -> isize;
        fn CFDataGetBytePtr(data: Object) -> *const u8;
        fn CFStringCreateWithCString(
            allocator: Object,
            string: *const std::ffi::c_char,
            encoding: u32,
        ) -> Object;
    }
    #[link(name = "ImageIO", kind = "framework")]
    unsafe extern "C" {
        fn CGImageDestinationCreateWithData(
            data: Object,
            image_type: Object,
            count: usize,
            options: Object,
        ) -> Object;
        fn CGImageDestinationAddImage(destination: Object, image: Object, properties: Object);
        fn CGImageDestinationFinalize(destination: Object) -> bool;
    }

    // Every Create/Copy result is owned and released once, including error paths.
    struct Owned(Object);
    impl Owned {
        fn new(object: Object) -> Result<Self, String> {
            if object.is_null() {
                Err("Window screenshot unavailable.".into())
            } else {
                Ok(Self(object))
            }
        }
        fn bytes(&self) -> Result<&[u8], String> {
            // Only called for CFData objects retained by this Owned value.
            unsafe {
                let length = usize::try_from(CFDataGetLength(self.0))
                    .map_err(|_| "Window screenshot unavailable.")?;
                let pointer = CFDataGetBytePtr(self.0);
                if length == 0 || length > 64 * 1024 * 1024 || pointer.is_null() {
                    return Err("Window screenshot unavailable.".into());
                }
                Ok(slice::from_raw_parts(pointer, length))
            }
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) }
        }
    }

    pub fn capture(number: u32) -> Result<Vec<u8>, String> {
        // IncludingWindow (1 << 3) restricts capture to the supplied app window.
        // CGRectNull uses that window's bounds; BoundsIgnoreFraming excludes its
        // shadow. This captures the composited Metal surface, unlike NSView draw.
        unsafe {
            let image = Owned::new(CGWindowListCreateImage(CGRectNull, 1 << 3, number, 1))?;
            let width = CGImageGetWidth(image.0);
            let height = CGImageGetHeight(image.0);
            if width == 0 || height == 0 || width > 4096 || height > 4096 {
                return Err("Window screenshot unavailable.".into());
            }
            let provider = CGImageGetDataProvider(image.0);
            if provider.is_null() {
                return Err("Window screenshot unavailable.".into());
            }
            let pixels = Owned::new(CGDataProviderCopyData(provider))?;
            // CoreGraphics may return transparent black when a window cannot be
            // read. Preserve diagnostic logs rather than attach a blank image.
            if pixels.bytes()?.iter().all(|byte| *byte == 0) {
                return Err("Window screenshot unavailable.".into());
            }
            let data = Owned::new(CFDataCreateMutable(ptr::null(), 0))?;
            let image_type = Owned::new(CFStringCreateWithCString(
                ptr::null(),
                c"public.png".as_ptr(),
                0x08000100,
            ))?;
            let destination = Owned::new(CGImageDestinationCreateWithData(
                data.0,
                image_type.0,
                1,
                ptr::null(),
            ))?;
            CGImageDestinationAddImage(destination.0, image.0, ptr::null());
            if !CGImageDestinationFinalize(destination.0) {
                return Err("Window screenshot unavailable.".into());
            }
            let png = data.bytes()?;
            if png.len() > 16 * 1024 * 1024 {
                return Err("Window screenshot unavailable.".into());
            }
            Ok(png.to_vec())
        }
    }
}
