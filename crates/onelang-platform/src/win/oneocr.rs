//! The OCR engine of the Windows 11 Snipping Tool (oneocr.dll). Much better
//! than Windows.Media.Ocr on screen text and multilingual by itself, but not a
//! public API: it is loaded from the installed Snipping Tool package and used
//! only when present.

use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use image::RgbaImage;
use windows::core::{HSTRING, PCSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HMODULE};
use windows::Win32::Storage::Packaging::Appx::{GetPackagePathByFullName, GetPackagesByPackageFamily};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};

use crate::{Error, Result};

const FAMILY: &str = "Microsoft.ScreenSketch_8wekyb3d8bbwe";
const FILES: [&str; 3] = ["oneocr.dll", "oneocr.onemodel", "onnxruntime.dll"];
/// The model file is encrypted; this is the key the Snipping Tool passes.
const MODEL_KEY: &[u8] = b"kj)TGtrK>f]b[Piow.gU+nC@s\"\"\"\"\"\"4\0";

#[repr(C)]
struct Image {
    kind: i32,
    width: i32,
    height: i32,
    reserved: i32,
    step: i64,
    data: *const u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Quad {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub x4: f32,
    pub y4: f32,
}

type Handle = i64;
type FnNew = unsafe extern "C" fn(*mut Handle) -> i64;
type FnSetDelayLoad = unsafe extern "C" fn(Handle, c_char) -> i64;
type FnCreatePipeline = unsafe extern "C" fn(*const c_char, *const c_char, Handle, *mut Handle) -> i64;
type FnSetMaxLines = unsafe extern "C" fn(Handle, i64) -> i64;
type FnRun = unsafe extern "C" fn(Handle, *const Image, Handle, *mut Handle) -> i64;
type FnCount = unsafe extern "C" fn(Handle, *mut i64) -> i64;
type FnItem = unsafe extern "C" fn(Handle, i64, *mut Handle) -> i64;
type FnContent = unsafe extern "C" fn(Handle, *mut *const c_char) -> i64;
type FnBox = unsafe extern "C" fn(Handle, *mut *const Quad) -> i64;
type FnRelease = unsafe extern "C" fn(Handle);

struct Api {
    run: FnRun,
    line_count: FnCount,
    line: FnItem,
    line_content: FnContent,
    line_box: FnBox,
    release_result: FnRelease,
}

struct Engine {
    api: Api,
    pipeline: Handle,
    options: Handle,
}

// The handles are only used under the mutex.
unsafe impl Send for Engine {}

pub struct Line {
    pub text: String,
    pub quad: Quad,
}

fn package_dir() -> Option<PathBuf> {
    unsafe {
        let family = HSTRING::from(FAMILY);
        let (mut count, mut len) = (0u32, 0u32);
        if GetPackagesByPackageFamily(&family, &mut count, None, &mut len, None) != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        let mut names = vec![PWSTR::null(); count as usize];
        let mut buf = vec![0u16; len as usize];
        if GetPackagesByPackageFamily(&family, &mut count, Some(names.as_mut_ptr()), &mut len, Some(PWSTR(buf.as_mut_ptr())))
            != ERROR_SUCCESS
        {
            return None;
        }
        // Several versions can be registered at once; the last one is the newest.
        for name in names.iter().rev() {
            let mut plen = 0u32;
            let _ = GetPackagePathByFullName(*name, &mut plen, None);
            let mut path = vec![0u16; plen as usize];
            if GetPackagePathByFullName(*name, &mut plen, Some(PWSTR(path.as_mut_ptr()))) == ERROR_SUCCESS {
                let path = String::from_utf16_lossy(&path[..path.iter().position(|&c| c == 0).unwrap_or(path.len())]);
                let dir = PathBuf::from(path).join("SnippingTool");
                if FILES.iter().all(|f| dir.join(f).exists()) {
                    return Some(dir);
                }
            }
        }
        None
    }
}

unsafe fn sym<T>(module: HMODULE, name: &str) -> Result<T> {
    let cname = CString::new(name).unwrap();
    let f = GetProcAddress(module, PCSTR(cname.as_ptr() as *const u8))
        .ok_or_else(|| Error::Other(format!("oneocr: {name} not found")))?;
    Ok(std::mem::transmute_copy(&f))
}

fn check(code: i64, what: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error::Other(format!("oneocr: {what} failed ({code})")))
    }
}

static WORK_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where the engine files are copied to: Windows does not let other apps load
/// DLLs straight from the WindowsApps folder.
pub fn set_work_dir(dir: PathBuf) {
    let _ = WORK_DIR.set(dir);
}

/// Copy the engine of the installed Snipping Tool version, dropping copies of older ones.
fn local_copy(source: &Path) -> Result<PathBuf> {
    let root = WORK_DIR.get().ok_or_else(|| Error::Other("oneocr: no work dir".into()))?;
    let version = source.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dest = root.join(&version);
    let io = |e: std::io::Error| Error::Other(format!("oneocr: {e}"));
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy() != version {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    std::fs::create_dir_all(&dest).map_err(io)?;
    for f in FILES {
        let (from, to) = (source.join(f), dest.join(f));
        let same = matches!((std::fs::metadata(&from), std::fs::metadata(&to)), (Ok(a), Ok(b)) if a.len() == b.len());
        if !same {
            // Through a temporary name: a half-copied file must not look complete.
            let tmp = dest.join(format!("{f}.part"));
            std::fs::copy(&from, &tmp).map_err(io)?;
            std::fs::rename(&tmp, &to).map_err(io)?;
        }
    }
    Ok(dest)
}

impl Engine {
    unsafe fn load() -> Result<Engine> {
        let source = package_dir().ok_or_else(|| Error::Other("Snipping Tool OCR is not installed".into()))?;
        let dir = local_copy(&source)?;
        // Altered search path: onnxruntime.dll next to it is picked up.
        let module = LoadLibraryExW(&HSTRING::from(dir.join("oneocr.dll").as_os_str()), None, LOAD_WITH_ALTERED_SEARCH_PATH)?;
        let new_init: FnNew = sym(module, "CreateOcrInitOptions")?;
        let set_delay_load: FnSetDelayLoad = sym(module, "OcrInitOptionsSetUseModelDelayLoad")?;
        let create_pipeline: FnCreatePipeline = sym(module, "CreateOcrPipeline")?;
        let new_options: FnNew = sym(module, "CreateOcrProcessOptions")?;
        let set_max_lines: FnSetMaxLines = sym(module, "OcrProcessOptionsSetMaxRecognitionLineCount")?;
        let api = Api {
            run: sym(module, "RunOcrPipeline")?,
            line_count: sym(module, "GetOcrLineCount")?,
            line: sym(module, "GetOcrLine")?,
            line_content: sym(module, "GetOcrLineContent")?,
            line_box: sym(module, "GetOcrLineBoundingBox")?,
            release_result: sym(module, "ReleaseOcrResult")?,
        };

        let mut init = 0;
        check(new_init(&mut init), "CreateOcrInitOptions")?;
        check(set_delay_load(init, 0), "OcrInitOptionsSetUseModelDelayLoad")?;
        let model = CString::new(dir.join("oneocr.onemodel").to_string_lossy().as_bytes())
            .map_err(|e| Error::Other(e.to_string()))?;
        let mut pipeline = 0;
        check(
            create_pipeline(model.as_ptr(), MODEL_KEY.as_ptr() as *const c_char, init, &mut pipeline),
            "CreateOcrPipeline",
        )?;
        let mut options = 0;
        check(new_options(&mut options), "CreateOcrProcessOptions")?;
        check(set_max_lines(options, 1000), "OcrProcessOptionsSetMaxRecognitionLineCount")?;
        Ok(Engine { api, pipeline, options })
    }

    unsafe fn recognize(&self, img: &RgbaImage) -> Result<Vec<Line>> {
        let mut bgra = img.as_raw().clone();
        for px in bgra.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let image = Image {
            kind: 3,
            width: img.width() as i32,
            height: img.height() as i32,
            reserved: 0,
            step: img.width() as i64 * 4,
            data: bgra.as_ptr(),
        };
        let mut result = 0;
        check((self.api.run)(self.pipeline, &image, self.options, &mut result), "RunOcrPipeline")?;
        let mut lines = vec![];
        let mut count = 0;
        if (self.api.line_count)(result, &mut count) == 0 {
            for i in 0..count {
                let mut line = 0;
                if (self.api.line)(result, i, &mut line) != 0 || line == 0 {
                    continue;
                }
                let mut text: *const c_char = std::ptr::null();
                if (self.api.line_content)(line, &mut text) != 0 || text.is_null() {
                    continue;
                }
                let mut quad: *const Quad = std::ptr::null();
                if (self.api.line_box)(line, &mut quad) != 0 || quad.is_null() {
                    continue;
                }
                lines.push(Line { text: CStr::from_ptr(text).to_string_lossy().into_owned(), quad: *quad });
            }
        }
        (self.api.release_result)(result);
        Ok(lines)
    }
}

static ENGINE: OnceLock<std::result::Result<Mutex<Engine>, String>> = OnceLock::new();

fn engine() -> Result<&'static Mutex<Engine>> {
    ENGINE
        .get_or_init(|| unsafe { Engine::load() }.map(Mutex::new).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| Error::Other(e.clone()))
}

/// Whether the Snipping Tool engine can be used (checked without loading it).
pub fn is_available() -> bool {
    match ENGINE.get() {
        Some(e) => e.is_ok(),
        None => package_dir().is_some(),
    }
}

/// Load (and on first use copy) the engine ahead of the first recognition.
pub fn warm_up() {
    if let Err(e) = engine() {
        log::warn!("Snipping Tool OCR: {e}");
    }
}

pub fn recognize(img: &RgbaImage) -> Result<Vec<Line>> {
    let engine = engine()?.lock().unwrap();
    unsafe { engine.recognize(img) }
}
